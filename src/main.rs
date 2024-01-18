mod ffi;

use crate::ffi::{DylibCommand, FatArch, FatHeader, MachHeader64};
use clap::Parser;
use crossterm::style::Stylize;
use inquire::Confirm;
use std::{
    fs::{metadata, File, OpenOptions},
    io::{Read, Seek, Write},
    path::Path,
    process::exit,
};

#[derive(Parser, Debug)]
#[command()]
struct Args {
    /// The input file to be modified
    input_file: String,
    /// The dylib path to be inserted
    dylib: String,
    /// Modify the input file in place
    #[arg(long, short)]
    inplace: bool,
    /// Run without asking for confirmation
    #[arg(long, short('y'))]
    all_yes: bool,
    /// Output path
    #[arg(short, conflicts_with = "inplace")]
    output_file: Option<String>,
}

macro_rules! read_buf {
    ($buf_reader:ident, $offset:expr, $type:ty) => {
        unsafe {
            $buf_reader.seek(std::io::SeekFrom::Start($offset)).unwrap();
            let mut buf: $type = std::mem::zeroed();
            $buf_reader
                .read_exact(std::slice::from_raw_parts_mut(
                    &mut buf as *mut _ as *mut u8,
                    std::mem::size_of::<$type>(),
                ))
                .unwrap();
            buf
        }
    };
}

fn main() {
    let args = Args::parse();

    if !Path::new(&args.input_file).exists() {
        eprintln!("Input file does not exist");
        exit(1);
    }

    if !metadata(&args.input_file).unwrap().is_file() {
        eprintln!("Input file is not a file");
        exit(1);
    }

    if !Path::new(&args.dylib).exists() {
        if !args.ask_for_confirmation(&format!(
            "Dylib file `{}` does not exist, continue?",
            args.dylib
        )) {
            exit(0);
        }
    }

    let patched_file = if args.inplace {
        if !args.ask_for_confirmation(&format!(
            "Input file `{}` will be modified in place, continue?",
            args.input_file
        )) {
            exit(0);
        }
        args.input_file.clone()
    } else {
        let patched_file = args
            .output_file
            .clone()
            .unwrap_or(format!("{}_patched", args.input_file));
        if Path::new(&patched_file).exists() {
            if !args.ask_for_confirmation(&format!(
                "Output file `{}` already exists, overwrite?",
                patched_file
            )) {
                exit(0);
            }
        }
        std::fs::copy(&args.input_file, &patched_file).unwrap();
        patched_file
    };

    let mut dylib = args.dylib.bytes().collect::<Vec<_>>();
    {
        let extra_space = 8 - dylib.len() % 8;
        dylib.append(&mut vec![0; extra_space]);
    }

    let mut file = OpenOptions::new()
        .read(true)
        .write(true)
        .open(&patched_file)
        .unwrap();

    let mut buf_reader = std::io::BufReader::new(&file);
    let magic = read_buf!(buf_reader, 0, u32);

    match magic {
        0xfeedfacf => {
            println!("match {} file", "64-bit Mach-O".red());
            args.insert_dylib(&mut file, 0x0, &dylib);
        }
        0xfeedface => {
            println!("match {} file", "32-bit Mach-O".red());
            eprintln!("32-bit Mach-O is not supported yet");
            exit(1);
        }
        0xcafebabe => {
            println!("match {} file", "fat_be".red());
            args.handle_fat(&mut file, &dylib, false);
        }
        0xbebafeca => {
            println!("match {} file", "fat_le".red());
            args.handle_fat(&mut file, &dylib, true);
        }
        magic => {
            eprintln!("Unknown magic num: {}", format!("{:#x}", magic).red());
            exit(1);
        }
    }

    println!("{}", "Done!".green().bold());
}

trait Utils {
    fn ask_for_confirmation(&self, msg: &str) -> bool;
    fn insert_dylib(&self, file: &mut File, global_offset: u64, dylib: &Vec<u8>);
    fn handle_fat(&self, file: &mut File, dylib: &Vec<u8>, le: bool);
}

impl Utils for Args {
    fn ask_for_confirmation(&self, msg: &str) -> bool {
        if self.all_yes {
            return true;
        }

        Confirm::new(msg).with_default(true).prompt().unwrap()
    }

    fn insert_dylib(&self, file: &mut File, global_offset: u64, dylib: &Vec<u8>) {
        let mut buf_reader = std::io::BufReader::new(&*file);
        let mut mach_header = read_buf!(buf_reader, 0x0 + global_offset, MachHeader64);

        let dylib_command = DylibCommand {
            cmd: 0x0c,
            cmdsize: 0x18 + dylib.len() as u32,
            name: 0x18,
            timestamp: 0,
            current_version: 0,
            compatibility_version: 0,
        };

        let mut buf_write: Vec<u8> = vec![];
        buf_write.extend(unsafe {
            std::slice::from_raw_parts(
                &dylib_command as *const _ as *const u8,
                std::mem::size_of::<DylibCommand>(),
            )
        });
        buf_write.append(&mut dylib.clone());

        println!(
            "writing at offset: {:#x}",
            0x20 + mach_header.sizeofcmds as u64 + global_offset
        );

        file.seek(std::io::SeekFrom::Start(
            0x20 + mach_header.sizeofcmds as u64 + global_offset,
        ))
        .unwrap();
        file.write_all(&buf_write).unwrap();

        println!("writing: `{:02x?}`", buf_write[0..0x18].to_vec());
        println!(
            "writing: `{}`",
            buf_write[0x18..]
                .iter()
                .map(|&b| {
                    if b == 0 {
                        '.'.negative().to_string()
                    } else {
                        (b as char).to_string()
                    }
                })
                .collect::<String>()
        );

        mach_header.sizeofcmds += dylib_command.cmdsize;
        mach_header.ncmds += 1;
        file.seek(std::io::SeekFrom::Start(0x0 + global_offset))
            .unwrap();
        file.write_all(unsafe {
            std::slice::from_raw_parts(
                &mach_header as *const _ as *const u8,
                std::mem::size_of::<MachHeader64>(),
            )
        })
        .unwrap();
    }

    fn handle_fat(&self, file: &mut File, dylib: &Vec<u8>, le: bool) {
        macro_rules! swap32 {
            ($num:expr) => {
                if le {
                    u32::from_be($num)
                } else {
                    $num
                }
            };
        }

        let mut buf_reader = std::io::BufReader::new(&*file);
        let fat_header = read_buf!(buf_reader, 0, FatHeader);
        let nfat_arch = swap32!(fat_header.nfat_arch);

        println!("find {} archs", nfat_arch);

        let not_ask = if nfat_arch == 0 {
            eprintln!("No arch found");
            exit(1);
        } else if nfat_arch > 1 {
            self.ask_for_confirmation("More than one arch found, insert dylib to all?")
        } else {
            true
        };

        for i in 0..nfat_arch {
            let mut buf_reader = std::io::BufReader::new(&*file);
            let fat_arch = read_buf!(buf_reader, 0x8 as u64 + i as u64 * 0x14, FatArch);

            match swap32!(fat_arch.cputype) {
                0x1000007 => {
                    println!("match {} arch", "x86_64".red());
                }
                0x100000c => {
                    println!("match {} arch", "arm64".red());
                }
                _ => {
                    eprintln!("Unsupported arch");
                    exit(1);
                }
            }

            if !not_ask && !self.ask_for_confirmation("Insert dylib?") {
                continue;
            }

            self.insert_dylib(file, swap32!(fat_arch.offset) as u64, &dylib);
        }
    }
}
