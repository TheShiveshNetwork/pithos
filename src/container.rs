use std::ffi::CString;

const MS_REC: u64 = 16384;
const MS_PRIVATE: u64 = 262144;

pub fn container_main(args: &Vec<String>) -> i32 {
    unsafe {
        let hostname = "pithos";
        let new_root = b"/tmp/pithos\0";
        let proc_dir = b"/tmp/pithos/proc\0";

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
        ) != 0
        {
            println!(
                "Failed to make mounts private: {}",
                std::io::Error::last_os_error()
            );
            return 1;
        }

        // The root filesystem must already exist before we chroot into it.
        if libc::mkdir(new_root.as_ptr() as *const libc::c_char, 0o755) != 0 {
            let error = std::io::Error::last_os_error();

            if error.raw_os_error() != Some(libc::EEXIST) {
                println!("Failed to create root filesystem: {}", error);
                return 1;
            }
        }

        if libc::mkdir(proc_dir.as_ptr() as *const libc::c_char, 0o555) != 0 {
            let error = std::io::Error::last_os_error();

            if error.raw_os_error() != Some(libc::EEXIST) {
                println!("Failed to create /proc: {}", error);
                return 1;
            }
        }

        let dirs_to_bind = [
            (b"/bin\0".as_ptr(), b"/tmp/pithos/bin\0".as_ptr()),
            (b"/lib\0".as_ptr(), b"/tmp/pithos/lib\0".as_ptr()),
            (b"/lib64\0".as_ptr(), b"/tmp/pithos/lib64\0".as_ptr()),
            (b"/usr\0".as_ptr(), b"/tmp/pithos/usr\0".as_ptr()),
            (b"/etc\0".as_ptr(), b"/tmp/pithos/etc\0".as_ptr()),
        ];

        for &(src, dest) in &dirs_to_bind {
            if libc::mkdir(dest as *const libc::c_char, 0o755) != 0 {
                let error = std::io::Error::last_os_error();

                if error.raw_os_error() != Some(libc::EEXIST) {
                    println!("Failed to create mount point: {}", error);
                    return 1;
                }
            }

            // Bind mount the host directory into the container rootfs.
            if libc::mount(
                src as *const libc::c_char,
                dest as *const libc::c_char,
                std::ptr::null(),
                libc::MS_BIND | libc::MS_REC,
                std::ptr::null(),
            ) != 0
            {
                println!("Failed to bind mount: {}", std::io::Error::last_os_error());
                return 1;
            }
        }

        // Change into the new root before calling chroot().
        // This is important because chroot() changes the process's filesystem root, but does not automatically change its cwd.
        if libc::chdir(new_root.as_ptr() as *const libc::c_char) != 0 {
            println!(
                "Failed to change directory to new root: {}",
                std::io::Error::last_os_error()
            );
            return 1;
        }

        // Make /tmp/pithos become / for this process.
        // We use "." because we are already inside /tmp/pithos.
        if libc::chroot(b".\0".as_ptr() as *const libc::c_char) != 0 {
            println!("chroot failed: {}", std::io::Error::last_os_error());
            return 1;
        }

        if libc::chdir(b"/\0".as_ptr() as *const libc::c_char) != 0 {
            println!(
                "Failed to change directory to new root: {}",
                std::io::Error::last_os_error()
            );
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
        ) != 0
        {
            println!("Failed to mount /proc: {}", std::io::Error::last_os_error());
            return 1;
        }

        let c_args: Vec<CString> = args
            .iter()
            .map(|s| CString::new(s.as_str()).unwrap())
            .collect();

        let mut argv: Vec<*const libc::c_char> = c_args.iter().map(|arg| arg.as_ptr()).collect();
        argv.push(std::ptr::null());

        // Set the shell prompt.
        std::env::set_var("PS1", "\\u@\\h:\\w\\$ ");

        libc::execvp(c_args[0].as_ptr(), argv.as_ptr());

        println!("execvp failed: {}", std::io::Error::last_os_error());
    }

    println!("Failed to execute command inside container");
    1
}
