use virtio_drivers::{Hal, PhysAddr, BufferDirection, PAGE_SIZE};
use core::ptr::NonNull;
use spin::Mutex;
use alloc::collections::BTreeMap;
use core::convert::TryInto;
use core::sync::atomic::{AtomicBool, Ordering};
use x86_64::{VirtAddr, PhysAddr as X86PhysAddr};

use crate::sys::mem;
use crate::sys::mem::PhysBuf;
use crate::{debug, error, eprint, eprintln, warning};

// global map to store PhysBuf instances
static DMA_BUFFERS: Mutex<BTreeMap<VirtAddr, PhysBuf>> = Mutex::new(BTreeMap::new());
// Flag to track if the static framebuffer has been "allocated" to the VirtIO GPU driver.
static IS_FB_ALLOCATED: AtomicBool = AtomicBool::new(false);

pub struct MyKernelHal;

unsafe impl Hal for MyKernelHal {
    // Allocates contiguous physical pages of DMA memory.
    fn dma_alloc(
        pages: usize,
        _direction: BufferDirection,
    ) -> (PhysAddr, NonNull<u8>) {
        let size = pages * PAGE_SIZE;
        //debug!("dma_alloc: Attempting to allocate {} bytes ({} pages) for DMA.", size, pages);

        // Special allocation for framebuffer which is greater than 1MB
        if size > 1 * 1024 * 1024 && !IS_FB_ALLOCATED.load(Ordering::Relaxed) {
            // Access static DMA_FRAMEBUFFER_REGION
            let fb_dma_buf = crate::sys::mem::dma_framebuffer();
            // Ensure the requested size fits within the pre-allocated 8MB buffer.
            if size <= fb_dma_buf.len() {
                // If the static framebuffer exists and is large enough, use it.
                IS_FB_ALLOCATED.store(true, Ordering::Relaxed);
                debug!("dma_alloc: Returning pre-allocated DMA_FRAMEBUFFER_REGION (size {}) for GPU framebuffer (requested {}).", fb_dma_buf.len(), size);
                return (fb_dma_buf.addr().try_into().unwrap(), NonNull::new(fb_dma_buf.as_mut_ptr()).unwrap());
            } else {
                // Fails if requested size is > 1MB but larger than the pre-allocated FB
                warning!("dma_alloc: Requested size {} exceeds pre-allocated DMA_FRAMEBUFFER_REGION size {}. Proceeding with dynamic allocation.", size, fb_dma_buf.len());
            }
        }

        // If not the specific framebuffer request or if DMA_FRAMEBUFFER_REGION not available
        // proceed with dynamic allocation for other DMA buffers.
        let mut phys_buf = PhysBuf::new(size);
        let phys_addr = phys_buf.addr();
        let virt_ptr = phys_buf.as_mut_ptr();
        let virt_addr = VirtAddr::from_ptr(virt_ptr);

        DMA_BUFFERS.lock().insert(virt_addr, phys_buf);
        (phys_addr.try_into().unwrap(), NonNull::new(virt_ptr).expect("dma_alloc: Virtual pointer was null"))
    }

    // Deallocates the given DMA memory region.
    unsafe fn dma_dealloc(
        paddr: PhysAddr,
        buffer: NonNull<u8>,
        _direction: usize,
    ) -> i32 {
        let virt_addr = VirtAddr::from_ptr(buffer.as_ptr());

        // Check if this is the pre-allocated framebuffer by physical address
        let fb_dma_buf = crate::sys::mem::dma_framebuffer();
        if fb_dma_buf.addr() == paddr as u64 {
            // Do not deallocate pre-allocated framebuffer
            debug!("dma_dealloc: Skipping deallocation for static DMA_FRAMEBUFFER_REGION.");
            IS_FB_ALLOCATED.store(false, Ordering::Relaxed); // Reset for potential re-initialization if driver is re-created
            return 0;
        }

        // Deallocation from DMA_BUFFERS.
        let mut dma_buffers = DMA_BUFFERS.lock();
        if let Some(_phys_buf) = dma_buffers.remove(&virt_addr) {
            debug!("dma_dealloc: Deallocated PhysBuf at virt {:?}", virt_addr);
            0
        } else {
            error!("dma_dealloc: Could not locate unmanaged or non-framebuffer memory at {:?}", virt_addr);
            -1
        }
    }

    // Converts MMIO physical address to virtual address.
    unsafe fn mmio_phys_to_virt(paddr: PhysAddr, _size: usize) -> NonNull<u8> {
        let virt_addr = mem::phys_to_virt(X86PhysAddr::new(paddr as u64));
        NonNull::new(virt_addr.as_mut_ptr())
            .expect("mmio_phys_to_virt: Converted virtual address was null, which should not happen for valid physical addresses.")
    }

    // Shares the given memory range with device.
    unsafe fn share(
        buffer: NonNull<[u8]>,
        _direction: BufferDirection,
    ) -> PhysAddr {
        let virt_ptr = buffer.as_ptr() as *const u8;
        mem::phys_addr(virt_ptr).try_into().unwrap()
    }

    // Unshares the given memory range from device.
    unsafe fn unshare(
        _paddr: PhysAddr,
        _buffer: NonNull<[u8]>,
        _direction: BufferDirection,
    ) {
        core::hint::black_box((_paddr, _buffer, _direction));
    }
}