// crates/systems/oxid_mac/src/memory.rs

/// Handles Macintosh RAM, including addressing modes, mirroring, and size-dependent behavior.
pub struct MacRam {
    data: Vec<u8>,
    size: usize,
    enable_mirroring: bool,
}

impl MacRam {
    /// Create a new MacRam instance with the specified size.
    /// Mirroring is automatically enabled for sizes < 1MB (Mac 128K/512K behavior).
    pub fn new(size: usize) -> Self {
        // Enforce Mac logic: < 1MB implies hardware mirroring wraparound
        // >= 1MB implies simpler decoding where valid RAM is contiguous and bounds are checked
        let enable_mirroring = size < 1024 * 1024;

        Self {
            data: vec![0; size],
            size,
            enable_mirroring,
        }
    }

    /// Read a byte from RAM.
    /// Handles mirroring or open bus (0xFF) behavior.
    pub fn read(&self, addr: u32) -> u8 {
        let addr_usize = addr as usize;

        if addr_usize < self.size {
            // Direct access within physical RAM
            // Safe unchecked because we checked bounds
            unsafe { *self.data.get_unchecked(addr_usize) }
        } else if self.enable_mirroring {
            // Mirroring for older Macs
            // Safe because logic implies size > 0
            self.data[addr_usize % self.size]
        } else {
            // Out of bounds read on Mac Plus/SE -> Open Bus
            0xFF
        }
    }

    /// Write a byte to RAM.
    /// Handles mirroring or ignores out of bounds writes.
    pub fn write(&mut self, addr: u32, value: u8) {
        let addr_usize = addr as usize;

        if addr_usize < self.size {
            // Direct access
            unsafe {
                *self.data.get_unchecked_mut(addr_usize) = value;
            }
        } else if self.enable_mirroring {
            // Mirroring
            self.data[addr_usize % self.size] = value;
        } else {
            // Out of bounds write on Mac Plus/SE -> Ignored
            // (In reality, writes to open bus do nothing)
        }
    }

    /// Direct access to the underlying buffer for Video/DMA, using standard slice.
    /// Returns the entire RAM buffer.
    pub fn dma_slice(&self) -> &[u8] {
        &self.data
    }

    /// Get RAM size
    #[allow(dead_code)]
    pub fn len(&self) -> usize {
        self.size
    }
}
