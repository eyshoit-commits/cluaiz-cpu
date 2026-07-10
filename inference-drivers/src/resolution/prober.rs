//! Hardware Prober: host OS, architecture and accelerator discovery.

use std::process::Command;
use sysinfo::System;
use tracing::{info, warn};

#[derive(Debug, Clone, serde::Serialize)]
pub struct ProbedHardware {
    pub os: String,
    pub arch: String,
    pub gpu_vendor: String,
    pub gpu_type: String,
    pub vram_gb: u64,
    pub cuda_version: Option<String>,
    pub hip_supported: bool,
}

pub struct HardwareProber;

impl HardwareProber {
    /// Probe the local machine without assuming that `sysinfo` exposes GPUs.
    pub fn probe() -> ProbedHardware {
        let mut system = System::new_all();
        system.refresh_all();

        let os = std::env::consts::OS.to_string();
        let arch = std::env::consts::ARCH.to_string();
        let (gpu_vendor, gpu_type, vram_gb) = Self::detect_gpu();
        let cuda_version = Self::detect_cuda_version();
        let hip_supported = Self::detect_hip_support();

        info!(
            os = %os,
            arch = %arch,
            gpu_vendor = %gpu_vendor,
            gpu_type = %gpu_type,
            vram_gb,
            total_memory = system.total_memory(),
            "Hardware probe completed"
        );

        ProbedHardware {
            os,
            arch,
            gpu_vendor,
            gpu_type,
            vram_gb,
            cuda_version,
            hip_supported,
        }
    }

    fn detect_gpu() -> (String, String, u64) {
        if let Some((name, memory_mib)) = Self::query_nvidia_smi() {
            info!(gpu = %name, "Detected NVIDIA GPU");
            return (
                "nvidia".to_string(),
                "gpu".to_string(),
                Self::mib_to_gib(memory_mib),
            );
        }

        #[cfg(target_os = "macos")]
        if let Some(chip) = Self::query_apple_gpu() {
            info!(gpu = %chip, "Detected Apple GPU");
            return ("apple".to_string(), "integrated_gpu".to_string(), 0);
        }

        #[cfg(target_os = "linux")]
        if let Some(description) = Self::query_linux_pci_gpu() {
            let normalized = description.to_ascii_lowercase();

            if normalized.contains("amd") || normalized.contains("ati") {
                info!(gpu = %description, "Detected AMD GPU");
                return ("amd".to_string(), "gpu".to_string(), 0);
            }

            if normalized.contains("intel") {
                info!(gpu = %description, "Detected Intel GPU");
                return ("intel".to_string(), "integrated_gpu".to_string(), 0);
            }

            if normalized.contains("nvidia") {
                info!(gpu = %description, "Detected NVIDIA GPU without nvidia-smi");
                return ("nvidia".to_string(), "gpu".to_string(), 0);
            }

            warn!(gpu = %description, "Detected GPU with unknown vendor");
            return ("generic".to_string(), "gpu".to_string(), 0);
        }

        ("generic".to_string(), "cpu".to_string(), 0)
    }

    fn query_nvidia_smi() -> Option<(String, u64)> {
        let output = Command::new("nvidia-smi")
            .args([
                "--query-gpu=name,memory.total",
                "--format=csv,noheader,nounits",
            ])
            .output()
            .ok()?;

        if !output.status.success() {
            return None;
        }

        let stdout = String::from_utf8(output.stdout).ok()?;
        let first_line = stdout.lines().find(|line| !line.trim().is_empty())?;
        let (name, memory) = first_line.split_once(',')?;
        let memory_mib = memory.trim().parse::<u64>().ok()?;

        Some((name.trim().to_string(), memory_mib))
    }

    #[cfg(target_os = "linux")]
    fn query_linux_pci_gpu() -> Option<String> {
        let output = Command::new("lspci").output().ok()?;

        if !output.status.success() {
            return None;
        }

        let stdout = String::from_utf8(output.stdout).ok()?;
        stdout
            .lines()
            .find(|line| {
                let line = line.to_ascii_lowercase();
                line.contains("vga compatible controller")
                    || line.contains("3d controller")
                    || line.contains("display controller")
            })
            .map(str::to_string)
    }

    #[cfg(target_os = "macos")]
    fn query_apple_gpu() -> Option<String> {
        let output = Command::new("system_profiler")
            .arg("SPDisplaysDataType")
            .output()
            .ok()?;

        if !output.status.success() {
            return None;
        }

        let stdout = String::from_utf8(output.stdout).ok()?;
        stdout
            .lines()
            .find_map(|line| line.trim().strip_prefix("Chipset Model:"))
            .map(|chip| chip.trim().to_string())
    }

    fn detect_cuda_version() -> Option<String> {
        let output = Command::new("nvidia-smi")
            .args(["--query-gpu=driver_version", "--format=csv,noheader"])
            .output()
            .ok()?;

        if !output.status.success() {
            return None;
        }

        let stdout = String::from_utf8(output.stdout).ok()?;
        stdout
            .lines()
            .find(|line| !line.trim().is_empty())
            .map(|version| version.trim().to_string())
    }

    fn detect_hip_support() -> bool {
        Command::new("rocminfo")
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
            || Command::new("rocm-smi")
                .arg("--showproductname")
                .output()
                .map(|output| output.status.success())
                .unwrap_or(false)
    }

    fn mib_to_gib(memory_mib: u64) -> u64 {
        memory_mib.saturating_add(1023) / 1024
    }
}
