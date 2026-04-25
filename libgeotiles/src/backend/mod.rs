pub mod cpu;
#[cfg(feature = "gpu")]
pub mod gpu;

/// Which backend is used to resample (crop + resize) tile pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ResampleBackend {
    #[default]
    Cpu,
    #[cfg(feature = "gpu")]
    Gpu,
}

#[cfg(test)]
mod tests;
