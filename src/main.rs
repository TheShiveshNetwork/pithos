use std::env;

mod container;
use container::container_main;

const CLONE_NEWNS: i32 = 0x00020000;
const CLONE_NEWUTS: i32 = 0x04000000;
const CLONE_NEWPID: i32 = 0x20000000;
const SIGCHLD: i32 = 17;

fn main() {
    let args: Vec<String> = env::args().collect();

    match args[1].as_str() {
        "run" => run_container(args[2..].to_vec()),
        _ => unimplemented!(),
    }
}

fn run_container(args: Vec<String>) {
    if args.is_empty() {
        println!("Error: No command provided to run");
    }

    const STACK_SIZE: usize = 1024 * 1024;
    let mut stack = vec![0u8; STACK_SIZE];

    // On x86_64, stacks grow downward, so we pass a pointer to the TOP of the allocated space
    let stack_top = unsafe { stack.as_mut_ptr().add(STACK_SIZE) as *mut libc::c_void };

    // Set up a closure/trampoline to pass into the low-level C function signature
    let mut user_data = args;

    extern "C" fn child_trampoline(arg: *mut libc::c_void) -> libc::c_int {
        let args_ptr = arg as *const Vec<String>;
        unsafe {
            container_main(&*args_ptr) as libc::c_int
        }
    }

    unsafe {
        let child_pid = libc::clone(
            child_trampoline,
            stack_top,
            CLONE_NEWUTS | CLONE_NEWPID | CLONE_NEWNS | SIGCHLD,
            &mut user_data as *mut _ as *mut libc::c_void,
        );

        if child_pid < 0 {
            panic!("Clone failed: {}", std::io::Error::last_os_error());
        }

        let mut status = 0;
        libc::waitpid(child_pid, &mut status, 0);
        println!("Container exited.");
    }
}

