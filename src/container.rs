use std::ffi::CString;

pub fn container_main(args: &Vec<String>) -> i32 {

    unsafe {
        let hostname = "pithos";

        if libc::sethostname(hostname.as_ptr() as *const libc::c_char, hostname.len()) != 0 {
            println!("Failed to set the hostname");
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
