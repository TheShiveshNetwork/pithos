use crate::network::configure_container_network;
use std::ffi::CString;

const MS_REC: u64 = 16384;
const MS_PRIVATE: u64 = 262144;
const MNT_DETACH: i32 = 2;

pub fn container_main(args: &Vec<String>, pipe_fd: libc::c_int) -> i32 {
    unsafe {
        let hostname = "pithos";
        let new_root = b"/tmp/pithos\0";
        let proc_dir = b"/tmp/pithos/proc\0";
        let old_root_relative = b"old_root\0";
        let old_root_absolute = b"/tmp/pithos/old_root\0";

        if libc::sethostname(hostname.as_ptr() as *const libc::c_char, hostname.len()) != 0 {
            println!("Failed to set hostname");
            return 1;
        }

        // Make mount namespace private
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

        if libc::mkdir(new_root.as_ptr() as *const libc::c_char, 0o755) != 0 {
            let error = std::io::Error::last_os_error();
            if error.raw_os_error() != Some(libc::EEXIST) {
                println!("Failed to create root filesystem: {}", error);
                return 1;
            }
        }

        // bind mount new_root onto itself to turn it into an official mount point
        if libc::mount(
            new_root.as_ptr() as *const libc::c_char,
            new_root.as_ptr() as *const libc::c_char,
            std::ptr::null(),
            libc::MS_BIND | libc::MS_REC,
            std::ptr::null(),
        ) != 0
        {
            println!(
                "Failed to self bind-mount new_root: {}",
                std::io::Error::last_os_error()
            );
            return 1;
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

        // Create the old_root directory inside new_root
        if libc::mkdir(old_root_absolute.as_ptr() as *const libc::c_char, 0o700) != 0 {
            let error = std::io::Error::last_os_error();
            if error.raw_os_error() != Some(libc::EEXIST) {
                println!("Failed to create old_root: {}", error);
                return 1;
            }
        }

        if libc::chdir(new_root.as_ptr() as *const libc::c_char) != 0 {
            println!(
                "Failed to chdir into new root: {}",
                std::io::Error::last_os_error()
            );
            return 1;
        }

        if libc::syscall(
            libc::SYS_pivot_root,
            b".\0".as_ptr() as *const libc::c_char,
            old_root_relative.as_ptr() as *const libc::c_char,
        ) != 0
        {
            println!("pivot_root failed: {}", std::io::Error::last_os_error());
            return 1;
        }

        if libc::chdir(b"/\0".as_ptr() as *const libc::c_char) != 0 {
            println!("Failed to chdir to /: {}", std::io::Error::last_os_error());
            return 1;
        }

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

        // Unmount old host root filesystem and remove directory
        if libc::umount2(b"/old_root\0".as_ptr() as *const libc::c_char, MNT_DETACH) != 0 {
            println!(
                "Failed to unmount old_root: {}",
                std::io::Error::last_os_error()
            );
        } else {
            libc::rmdir(b"/old_root\0".as_ptr() as *const libc::c_char);
        }

        // BLOCK UNTIL PARENT FINISHES SETTING UP VETH PAIR
        let mut sync_buf = [0u8; 1];
        libc::read(pipe_fd, sync_buf.as_mut_ptr() as *mut libc::c_void, 1);
        libc::close(pipe_fd);

        // Configure network inside container namespace
        if let Err(error) = configure_container_network() {
            println!("Failed to configure container network: {:?}", error);
            return 1;
        }

        let c_args: Vec<CString> = args
            .iter()
            .map(|s| CString::new(s.as_str()).unwrap())
            .collect();

        let mut argv: Vec<*const libc::c_char> = c_args.iter().map(|arg| arg.as_ptr()).collect();
        argv.push(std::ptr::null());

        std::env::set_var("PS1", "\\u@\\h:\\w\\$ ");

        libc::execvp(c_args[0].as_ptr(), argv.as_ptr());
        println!("execvp failed: {}", std::io::Error::last_os_error());
    }

    1
}
