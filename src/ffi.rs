#[repr(C)]
pub struct MachHeader64 {
    pub magic: u32,
    pub cpu_type: i32,
    pub cpu_sub_type: i32,
    pub filetype: u32,
    pub ncmds: u32,
    pub sizeofcmds: u32,
    pub flags: u32,
    pub reserved: u32,
}

#[repr(C)]
pub struct DylibCommand {
    pub cmd: u32,
    pub cmdsize: u32,
    pub name: u32,
    pub timestamp: u32,
    pub current_version: u32,
    pub compatibility_version: u32,
}

#[repr(C)]
pub struct FatHeader {
    pub magic: u32,
    pub nfat_arch: u32,
}

#[repr(C)]
pub struct FatArch {
    pub cputype: u32,
    pub cpusubtype: u32,
    pub offset: u32,
    pub size: u32,
    pub align: u32,
}
