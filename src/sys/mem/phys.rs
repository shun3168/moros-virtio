use alloc::slice::SliceIndex;
use alloc::sync::Arc;
use alloc::vec;
use alloc::vec::Vec;
use core::ops::{Index, IndexMut};
use spin::Mutex;

// Modified by shshi102
use x86_64::{VirtAddr, PhysAddr as X86PhysAddr};

#[derive(Clone)]
pub struct PhysBuf {
    buf: Arc<Mutex<Vec<u8>>>,
}

impl PhysBuf {
    pub fn new(len: usize) -> Self {
        Self::from(vec![0; len])
    }

    // Realloc vec until it uses a chunk of contiguous physical memory
    fn from(vec: Vec<u8>) -> Self {
        let buffer_end = vec.len() - 1;
        let memory_end = phys_addr(&vec[buffer_end]) - phys_addr(&vec[0]);
        if buffer_end == memory_end as usize {
            Self {
                buf: Arc::new(Mutex::new(vec)),
            }
        } else {
            Self::from(vec.clone()) // Clone vec and try again
        }
    }

    pub fn addr(&self) -> u64 {
        phys_addr(&self.buf.lock()[0])
    }
}

impl<I: SliceIndex<[u8]>> Index<I> for PhysBuf {
    type Output = I::Output;

    #[inline]
    fn index(&self, index: I) -> &Self::Output {
        Index::index(&**self, index)
    }
}

impl<I: SliceIndex<[u8]>> IndexMut<I> for PhysBuf {
    #[inline]
    fn index_mut(&mut self, index: I) -> &mut Self::Output {
        IndexMut::index_mut(&mut **self, index)
    }
}

impl core::ops::Deref for PhysBuf {
    type Target = [u8];

    fn deref(&self) -> &[u8] {
        let vec = self.buf.lock();
        unsafe { alloc::slice::from_raw_parts(vec.as_ptr(), vec.len()) }
    }
}

impl core::ops::DerefMut for PhysBuf {
    fn deref_mut(&mut self) -> &mut [u8] {
        let mut vec = self.buf.lock();
        unsafe {
            alloc::slice::from_raw_parts_mut(vec.as_mut_ptr(), vec.len())
        }
    }
}

pub fn phys_addr(ptr: *const u8) -> u64 {
    let virt_addr = VirtAddr::new(ptr as u64);
    let phys_addr = super::virt_to_phys(virt_addr).unwrap();
    phys_addr.as_u64()
}


// Modified by shshi102
// A buffer specifically for a pre-reserved, physically contiguous DMA region.
// This buffer holds references to the physical and virtual addresses, and its size.
// Not perform allocations itself, but manages an already allocated/mapped region.
#[derive(Debug)]
pub struct DmaPhysBuf {
    phys_start: X86PhysAddr,
    virt_start: VirtAddr,
    size: usize,
}

// Modified by shshi102
impl DmaPhysBuf {
    /// Creates a new `DmaPhysBuf` instance for an already mapped contiguous region.
    pub unsafe fn new(
        phys_start: X86PhysAddr,
        virt_start: VirtAddr,
        size: usize,
    ) -> Self {
        DmaPhysBuf {
            phys_start,
            virt_start,
            size,
        }
    }

    /// Returns the starting physical address of the DMA buffer.
    pub fn addr(&self) -> u64 {
        self.phys_start.as_u64()
    }

    /// Returns a mutable pointer to the starting virtual address of the DMA buffer.
    pub fn as_mut_ptr(&self) -> *mut u8 {
        self.virt_start.as_mut_ptr()
    }

    /// Returns the size of the DMA buffer in bytes.
    pub fn len(&self) -> usize {
        self.size
    }

    /// Returns a mutable slice over the entire DMA buffer.
    pub unsafe fn as_mut_slice(&mut self) -> &mut [u8] {
        core::slice::from_raw_parts_mut(self.as_mut_ptr(), self.len())
    }
}