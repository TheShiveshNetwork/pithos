use std::env;
use std::process::Command;

fn main() {
    let args: Vec<String> = env::args().collect();

    match args[1].as_str() {
        "run" => run(args[2..].to_vec()),
        _ => unimplemented!(),
    }
}

fn run(args: Vec<String>) {

    let mut child = Command::new(&args[0])
        .args(&args[1..])
        .spawn()
        .expect("Failed to start the process");

    let status = child.wait()
                    .expect("Failed to wait on child process");

    if !status.success() {
        println!("Process exited with an error: {:?}", status.code());
    }
}

