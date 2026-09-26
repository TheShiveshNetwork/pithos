use std::env;

mod container;
use container::container_main;

mod network;
use network::setup_network;

const CLONE_NEWNS: i32 = 0x00020000;
const CLONE_NEWUTS: i32 = 0x04000000;
const CLONE_NEWPID: i32 = 0x20000000;
const CLONE_NEWNET: i32 = 0x40000000;
const SIGCHLD: i32 = 17;

struct ContainerArgs {
    args: Vec<String>,
    pipe_read_fd: libc::c_int,
}

fn main() {
    let args: Vec<String> = env::args().collect();

    match args.get(1).map(|s| s.as_str()) {
        Some("run") => run_container(args[2..].to_vec()),
        _ => println!("Usage: {} run <command>", args[0]),
    }
}

fn run_container(args: Vec<String>) {
    if args.is_empty() {
        println!("Error: No command provided to run");
        return;
    }

    // 1. Create synchronization pipe
    let mut pipe_fds: [libc::c_int; 2] = [0, 0];
    unsafe {
        if libc::pipe(pipe_fds.as_mut_ptr()) != 0 {
            panic!("Failed to create pipe: {}", std::io::Error::last_os_error());
        }
    }

    const STACK_SIZE: usize = 1024 * 1024;
    let mut stack = vec![0u8; STACK_SIZE];
    let stack_top = unsafe { stack.as_mut_ptr().add(STACK_SIZE) as *mut libc::c_void };

    let mut user_data = ContainerArgs {
        args,
        pipe_read_fd: pipe_fds[0],
    };

    extern "C" fn child_trampoline(arg: *mut libc::c_void) -> libc::c_int {
        let container_args = unsafe { &*(arg as *const ContainerArgs) };
        unsafe {
            // Close unneeded write end in child
            libc::close(container_args.pipe_read_fd + 1);
            container_main(&container_args.args, container_args.pipe_read_fd) as libc::c_int
        }
    }

    unsafe {
        let child_pid = libc::clone(
            child_trampoline,
            stack_top,
            CLONE_NEWUTS | CLONE_NEWPID | CLONE_NEWNS | CLONE_NEWNET | SIGCHLD,
            &mut user_data as *mut _ as *mut libc::c_void,
        );

        if child_pid < 0 {
            panic!("Clone failed: {}", std::io::Error::last_os_error());
        }

        // Parent closes the read end
        libc::close(pipe_fds[0]);

        // 2. Setup host side network & move interface to child namespace
        if let Err(error) = setup_network(child_pid) {
            panic!("Failed to setup container network: {:?}", error);
        }

        // 3. Signal child that network is ready
        let sync_signal: u8 = 1;
        libc::write(
            pipe_fds[1],
            &sync_signal as *const u8 as *const libc::c_void,
            1,
        );
        libc::close(pipe_fds[1]);

        let mut status = 0;
        libc::waitpid(child_pid, &mut status, 0);
        println!("Container exited.");
    }
}
