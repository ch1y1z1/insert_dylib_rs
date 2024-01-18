#[link_section = "__DATA,__mod_init_func"]
static inject: unsafe extern "C" fn() = {
    extern "C" fn inject() {
        println!("Hello from injected code!");
    }
    inject
};
