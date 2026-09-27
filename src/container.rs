use crate::network::configure_container_network;
use std::ffi::CString;

use crate::filesystem;

const MS_REC: u64 = 16384;
const MS_PRIVATE: u64 = 262144;
const MNT_DETACH: i32 = 2;

fn run_init_reaper(args: &Vec<String>) -> i32 {
    unsafe {
        let target_pid = libc::fork();

        if target_pid < 0 {
            println!(
                "Fork failed inside init process: {}",
                std::io::Error::last_os_error()
            );
            return 1;
        }

        if target_pid == 0 {
            let deafult_cmd = vec!["/bin/sh".to_string()];
            let cmd_args = if args.is_empty() { &deafult_cmd } else { args };

            let c_args: Vec<CString> = args
                .iter()
                .map(|s| CString::new(s.as_str()).unwrap())
                .collect();

            let mut argv: Vec<*const libc::c_char> =
                c_args.iter().map(|arg| arg.as_ptr()).collect();
            argv.push(std::ptr::null());

            // Set standard container environment variables
            std::env::set_var(
                "PATH",
                "/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin",
            );
            std::env::set_var("TERM", "xterm");
            std::env::set_var("HOME", "/root");
            std::env::set_var("PS1", "\\u@\\h:\\w\\$ ");

            libc::execvp(c_args[0].as_ptr(), argv.as_ptr());

            println!(
                "execvp failed for command '{:?}': {}",
                cmd_args[0],
                std::io::Error::last_os_error()
            );
            libc::_exit(1);
        }

        let mut exit_code = 0;

        loop {
            let mut status: libc::c_int = 0;

            let reaped_pid = libc::waitpid(-1, &mut status, 0);

            if reaped_pid <= 0 {
                let err = std::io::Error::last_os_error();
                if err.raw_os_error() == Some(libc::ECHILD) {
                    break;
                }
                continue;
            }

            if reaped_pid == target_pid {
                if libc::WIFEXITED(status) {
                    exit_code = libc::WEXITSTATUS(status);
                } else if libc::WIFSIGNALED(status) {
                    exit_code = 128 + libc::WTERMSIG(status);
                }
            }
        }

        exit_code
    }
}

pub fn setup_overlayfs() -> Result<(), Box<dyn std::error::Error>> {
    filesystem::ensure_base_rootfs()?;

    unsafe {
        let upper = b"/tmp/pithos/upper\0";
        let work = b"/tmp/pithos/work\0";
        let merged = b"/tmp/pithos/rootfs\0";

        let dirs: [&[u8]; 3] = [upper, work, merged];

        for dir in dirs {
            if libc::mkdir(dir.as_ptr() as *const libc::c_char, 0o755) != 0 {
                let err = std::io::Error::last_os_error();
                if err.raw_os_error() != Some(libc::EEXIST) {
                    return Err(Box::new(err));
                }
            }
        }

        let opts = CString::new(
            "lowerdir=/tmp/pithos/base,upperdir=/tmp/pithos/upper,workdir=/tmp/pithos/work",
        )?;

        let fstype = b"overlay\0";

        if libc::mount(
            fstype.as_ptr() as *const libc::c_char,
            merged.as_ptr() as *const libc::c_char,
            fstype.as_ptr() as *const libc::c_char,
            0,
            opts.as_ptr() as *const libc::c_void,
        ) != 0
        {
            return Err(Box::new(std::io::Error::last_os_error()));
        }
    }

    Ok(())
}

pub fn container_main(args: &Vec<String>, pipe_fd: libc::c_int) -> i32 {
    unsafe {
        let hostname = "pithos";
        let merged_root = b"/tmp/pithos/rootfs\0";
        let proc_dir = b"/tmp/pithos/rootfs/proc\0";
        let old_root = b"old_root\0";
        let old_root_dir = [b"/tmp/pithos/rootfs/".as_slice(), old_root].concat();

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

        // setup OverlayFS (mounts base + upper onto /tmp/pithos/rootfs)
        if let Err(err) = setup_overlayfs() {
            println!("Failed to setup OverlayFS: {}", err);
            return 1;
        }

        // Bind-mount target root to satisfy pivot_root requirement
        if libc::mount(
            merged_root.as_ptr() as *const libc::c_char,
            merged_root.as_ptr() as *const libc::c_char,
            std::ptr::null(),
            libc::MS_BIND | libc::MS_REC,
            std::ptr::null(),
        ) != 0
        {
            println!(
                "Failed to bind-mount merged_root: {}",
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

        // Create the old_root directory inside merged_root
        if libc::mkdir(old_root_dir.as_ptr() as *const libc::c_char, 0o700) != 0 {
            let error = std::io::Error::last_os_error();
            if error.raw_os_error() != Some(libc::EEXIST) {
                println!("Failed to create old_root: {}", error);
                return 1;
            }
        }

        if libc::chdir(merged_root.as_ptr() as *const libc::c_char) != 0 {
            println!(
                "Failed to chdir into new root: {}",
                std::io::Error::last_os_error()
            );
            return 1;
        }

        if libc::syscall(
            libc::SYS_pivot_root,
            b".\0".as_ptr() as *const libc::c_char,
            old_root.as_ptr() as *const libc::c_char,
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
        if libc::umount2(old_root.as_ptr() as *const libc::c_char, MNT_DETACH) != 0 {
            println!(
                "Failed to unmount old_root: {}",
                std::io::Error::last_os_error()
            );
        } else {
            libc::rmdir(old_root.as_ptr() as *const libc::c_char);
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

        run_init_reaper(args)
    }
}
