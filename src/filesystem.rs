use flate2::read::GzDecoder;
use std::fs;
use std::path::Path;
use tar::Archive;

static EMBEDDED_ROOTFS: &[u8] = include_bytes!("../embed/alpine-rootfs.tar.gz");

pub fn ensure_base_rootfs() -> Result<(), Box<dyn std::error::Error>> {
    let base_dir = Path::new("/tmp/pithos/base");

    // Check if base rootfs is already extracted
    if base_dir.exists() && base_dir.join("bin/sh").exists() {
        return Ok(());
    }

    fs::create_dir_all(base_dir)?;

    let tar = GzDecoder::new(EMBEDDED_ROOTFS);
    let mut archive = Archive::new(tar);
    archive.unpack(base_dir)?;

    Ok(())
}
