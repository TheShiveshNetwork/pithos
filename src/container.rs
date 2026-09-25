use std::ffi::CString;

const MS_REC: u64 = 16384;
const MS_PRIVATE: u64 = 262144;

pub fn container_main(args: &Vec<String>) -> i32 {

    unsafe {
        let hostname = "pithos";

        if libc::sethostname(hostname.as_ptr() as *const libc::c_char, hostname.len()) != 0 {
            println!("Failed to set the hostname");
            return 1;
        }

        // make mount namespace changes private so they don't leak back to the host
        if libc::mount(
            std::ptr::null(),
            b"/\0".as_ptr() as *const libc::c_char,
            std::ptr::null(),
            MS_REC | MS_PRIVATE,
            std::ptr::null(),
        ) != 0 {
            println!("Failed to make mounts private: {}", std::io::Error::last_os_error());
            return 1;
        }

        // In Rust, string slices are not null-terminated (\0). When you cast this pointer to a *const libc::c_char and pass it to libc::mount, 
        // the underlying C function keeps reading memory past the / character until it randomly encounters a zero byte. To fix thix, we need to
        // manually add a \0 null character to the end of the string
        let source = b"proc\0";
        let target = b"/proc\0";
        let fstype = b"proc\0";

        if libc::mount(
            source.as_ptr() as *const libc::c_char,
            target.as_ptr() as *const libc::c_char,
            fstype.as_ptr() as *const libc::c_char,
            0,
            std::ptr::null(),
        ) != 0 {
            println!("Failed to mount /proc: {}", std::io::Error::last_os_error());
            return 1;
        }

        let c_command = CString::new(args[0].as_str()).expect("CString conversion failed");

        let mut c_args: Vec<*const libc::c_char> = args
            .iter()
            .map(|s| CString::new(s.as_str()).unwrap().into_raw() as *const libc::c_char)
            .collect();
        c_args.push(std::ptr::null());

        libc::execvp(c_command.as_ptr(), c_args.as_ptr());
    }

    println!("Failed to execute command inside container");
    1
}
