use std::fs;
use std::path::{Path, PathBuf};

pub struct CgroupManager {
    path: PathBuf,
}

impl CgroupManager {
    /// Creates a new cgroup under /sys/fs/cgroup for the given child PID
    pub fn new(child_pid: libc::pid_t) -> Result<Self, std::io::Error> {
        let cgroup_name = format!("pithos_{}", child_pid);
        let path = Path::new("/sys/fs/cgroup").join(cgroup_name);

        if !path.exists() {
            fs::create_dir_all(&path)?;
        }

        Ok(Self { path })
    }

    /// Attaches the child process PID to this cgroup
    pub fn attach_pid(&self, pid: libc::pid_t) -> Result<(), std::io::Error> {
        let procs_path = self.path.join("cgroup.procs");
        fs::write(procs_path, pid.to_string())?;
        Ok(())
    }

    /// Limits maximum memory in bytes (e.g., 512MB = 536870912)
    pub fn set_memory_limit(&self, max_bytes: usize) -> Result<(), std::io::Error> {
        let mem_path = self.path.join("memory.max");
        fs::write(mem_path, max_bytes.to_string())?;
        Ok(())
    }

    /// Limits CPU usage using quota/period (e.g., quota=50000, period=100000 gives 0.5 CPU cores)
    pub fn set_cpu_limit(&self, quota_us: u64, period_us: u64) -> Result<(), std::io::Error> {
        let cpu_path = self.path.join("cpu.max");
        let limit_str = format!("{} {}", quota_us, period_us);
        fs::write(cpu_path, limit_str)?;
        Ok(())
    }

    /// Limits maximum number of processes to prevent PID exhaustion/fork bombs
    pub fn set_pids_limit(&self, max_pids: u32) -> Result<(), std::io::Error> {
        let pids_path = self.path.join("pids.max");
        fs::write(pids_path, max_pids.to_string())?;
        Ok(())
    }
}

impl Drop for CgroupManager {
    fn drop(&mut self) {
        // Automatically delete the cgroup directory when the CgroupManager instance goes out of scope
        if self.path.exists() {
            let _ = fs::remove_dir(&self.path);
        }
    }
}
