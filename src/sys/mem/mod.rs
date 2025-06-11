mod heap;
mod paging;
mod phys;

pub use paging::{alloc_pages, free_pages, active_page_table, create_page_table};

// DmaPhysBuf for framebuffer, Modified by shshi102
pub use phys::{phys_addr, PhysBuf, DmaPhysBuf};

use crate::sys;
use bootloader::bootinfo::{BootInfo, MemoryMap, MemoryRegionType};
use core::sync::atomic::{AtomicUsize, Ordering};
use spin::Once;
use x86_64::structures::paging::{
    FrameAllocator, OffsetPageTable, PhysFrame, Size4KiB, Translate, PageSize,
};
use x86_64::{PhysAddr, VirtAddr};

// Modified by shshi102
use spin::Mutex;

// Define a static Mutex-protected instance of the FrameAllocator, Modified by shshi102
static GLOBAL_FRAME_ALLOCATOR: Once<Mutex<BootInfoFrameAllocator>> = Once::new();
pub struct GlobalFrameAllocatorRef; // Special allocator reference type that always locks the global allocator.

// Modified by shshi102
unsafe impl FrameAllocator<Size4KiB> for GlobalFrameAllocatorRef {
    fn allocate_frame(&mut self) -> Option<PhysFrame> {
        let mut allocator_guard = GLOBAL_FRAME_ALLOCATOR.get()
            .expect("Global Frame Allocator not initialized!")
            .lock();
        allocator_guard.allocate_frame()
    }
}

// Modified by shshi102
pub fn frame_allocator() -> GlobalFrameAllocatorRef {
    GlobalFrameAllocatorRef
}

#[allow(static_mut_refs)]
static mut MAPPER: Once<OffsetPageTable<'static>> = Once::new();

static PHYS_MEM_OFFSET: Once<u64> = Once::new();
static MEMORY_MAP: Once<&MemoryMap> = Once::new();
static MEMORY_SIZE: AtomicUsize = AtomicUsize::new(0);

// Modified by shshi102
//static ALLOCATED_FRAMES: AtomicUsize = AtomicUsize::new(0);

// Modified by shshi102
static FRAMEBUFFER_PHYS_RANGE: Mutex<Option<(PhysAddr, PhysAddr)>> = Mutex::new(None);
pub static DMA_FRAMEBUFFER_REGION: Once<DmaPhysBuf> = Once::new();
pub fn dma_framebuffer() -> &'static DmaPhysBuf {
    DMA_FRAMEBUFFER_REGION.get().expect("DMA Framebuffer not initialized!")
}
const FRAMEBUFFER_SIZE_IN_BYTES: usize = 8 * 1024 * 1024; // 8 MB
const FRAMEBUFFER_VIRT_START: VirtAddr = VirtAddr::new(0xFFFF_FF00_0000_0000);

pub fn init(boot_info: &'static BootInfo) {
    // Keep the timer interrupt to have accurate boot time measurement but mask
    // the keyboard interrupt that would create a panic if a key is pressed
    // during memory allocation otherwise.
    sys::idt::set_irq_mask(1);

    let mut memory_size = 0;
    let mut last_end_addr = 0;
    for region in boot_info.memory_map.iter() {
        let start_addr = region.range.start_addr();
        let end_addr = region.range.end_addr();
        let size = end_addr - start_addr;
        let hole = start_addr - last_end_addr;
        if hole > 0 {
            log!(
                "MEM [{:#016X}-{:#016X}] {}", // "({} KB)"
                last_end_addr, start_addr - 1, "Unmapped" //, hole >> 10
            );
            if start_addr < (1 << 20) {
                memory_size += hole; // BIOS memory
            }
        }
        log!(
            "MEM [{:#016X}-{:#016X}] {:?}", // "({} KB)"
            start_addr, end_addr - 1, region.region_type //, size >> 10
        );
        memory_size += size;
        last_end_addr = end_addr;
    }

    // FIXME: There are two small reserved areas at the end of the physical
    // memory that should be removed from the count to be fully accurate but
    // their sizes and location vary depending on the amount of RAM on the
    // system. It doesn't affect the count in megabytes.
    log!("RAM {} MB", memory_size >> 20);
    MEMORY_SIZE.store(memory_size as usize, Ordering::Relaxed);

    #[allow(static_mut_refs)]
    unsafe {
        MAPPER.call_once(|| OffsetPageTable::new(
            paging::active_page_table(),
            VirtAddr::new(boot_info.physical_memory_offset),
        ))
    };

    PHYS_MEM_OFFSET.call_once(|| boot_info.physical_memory_offset);
    MEMORY_MAP.call_once(|| &boot_info.memory_map);

    // Before heap initialization, reserve DMA Framebuffer physical region, Modified by shshi102
    log!("Initializing framebuffer memory region...");
    let framebuffer_phys_start = {
        let mut found_start = None;
        for region in boot_info.memory_map.iter() {
            if region.region_type == MemoryRegionType::Usable && region.range.end_addr() - region.range.start_addr() >= FRAMEBUFFER_SIZE_IN_BYTES as u64 {
                let start_addr = PhysAddr::new(region.range.start_addr());
                if start_addr.is_aligned(Size4KiB::SIZE) {
                    found_start = Some(start_addr);
                    break;
                }
            }
        }
        found_start.expect("Could not find a suitable physical memory region for the framebuffer!")
    };
    log!("Found physical memory for framebuffer at: {:#x}", framebuffer_phys_start.as_u64());
    let framebuffer_phys_end = framebuffer_phys_start + FRAMEBUFFER_SIZE_IN_BYTES as u64;
    FRAMEBUFFER_PHYS_RANGE.lock().replace((framebuffer_phys_start, framebuffer_phys_end));
    log!("Reserved framebuffer physical range: {:#x}-{:#x}", framebuffer_phys_start.as_u64(), framebuffer_phys_end.as_u64());

    // Initialize the global frame allocator, Modified by shshi102
    unsafe {
        GLOBAL_FRAME_ALLOCATOR.call_once(|| {
            Mutex::new(BootInfoFrameAllocator::init(MEMORY_MAP.get_unchecked()))
        });
    }

    // Map the contiguous physical framebuffer region to the chosen virtual address, Modified by shshi102
    unsafe {
        paging::map_contiguous_physical_region(
            mapper(),
            framebuffer_phys_start,
            FRAMEBUFFER_VIRT_START,
            FRAMEBUFFER_SIZE_IN_BYTES,
        )
    }.expect("Failed to map framebuffer physical region to virtual address!");

    // Initialize DMA Framebuffer Region, Modified by shshi102
    DMA_FRAMEBUFFER_REGION.call_once(|| unsafe {
        DmaPhysBuf::new(
            framebuffer_phys_start,
            FRAMEBUFFER_VIRT_START,
            FRAMEBUFFER_SIZE_IN_BYTES,
        )
    });
    log!("DMA Framebuffer region initialized.");

    heap::init_heap().expect("heap initialization failed");

    sys::idt::clear_irq_mask(1);
}

pub fn phys_mem_offset() -> u64 {
    unsafe { *PHYS_MEM_OFFSET.get_unchecked() }
}

pub fn mapper() -> &'static mut OffsetPageTable<'static> {
    #[allow(static_mut_refs)]
    unsafe { MAPPER.get_mut_unchecked() }
}

pub fn memory_size() -> usize {
    MEMORY_SIZE.load(Ordering::Relaxed)
}

pub fn memory_used() -> usize {
    (memory_size() - heap::heap_size()) + heap::heap_used()
}

pub fn memory_free() -> usize {
    heap::heap_free()
}

pub fn phys_to_virt(addr: PhysAddr) -> VirtAddr {
    VirtAddr::new(addr.as_u64() + phys_mem_offset())
}

pub fn virt_to_phys(addr: VirtAddr) -> Option<PhysAddr> {
    mapper().translate_addr(addr)
}

// Modified by shshi102
pub struct BootInfoFrameAllocator {
    memory_map: &'static MemoryMap,
    current_region_idx: usize,
    current_frame_offset: u64,
}

// Modified by shshi102
impl BootInfoFrameAllocator {
    pub unsafe fn init(memory_map: &'static MemoryMap) -> Self {
        BootInfoFrameAllocator {
            memory_map,
            current_region_idx: 0,
            current_frame_offset: 0,
        }
    }

    fn is_frame_usable(&self, frame_addr: PhysAddr) -> bool {
        let reserved_range_guard = FRAMEBUFFER_PHYS_RANGE.lock();
        let reserved_range_copy = *reserved_range_guard;

        if let Some((fb_start, fb_end)) = reserved_range_copy {
            !(frame_addr >= fb_start && frame_addr < fb_end)
        } else {
            true
        }
    }
}

// Memory allocation logic has changed to adjust DMA for Framebuffer, Modified by shshi102
unsafe impl FrameAllocator<Size4KiB> for BootInfoFrameAllocator {
    fn allocate_frame(&mut self) -> Option<PhysFrame> {
        loop {
            let current_region_option = self.memory_map.iter().nth(self.current_region_idx);
            let current_region = match current_region_option {
                Some(region) => region,
                None => {
                    //log!("DEBUG: FrameAllocator: No more memory regions to check.");
                    return None; // No more frames available
                }
            };

            if current_region.region_type != MemoryRegionType::Usable {
                //log!("DEBUG: FrameAllocator: Skipping non-usable region at index {}: {:?}", self.current_region_idx, current_region.region_type);
                self.current_region_idx += 1;
                self.current_frame_offset = 0;
                continue;
            }

            let region_start = current_region.range.start_addr();
            let region_end = current_region.range.end_addr();

            // Calculate the physical address of the potential next frame
            let potential_frame_addr = PhysAddr::new(region_start + self.current_frame_offset);

            if potential_frame_addr.as_u64() < region_end {
                let frame = PhysFrame::containing_address(potential_frame_addr);
                //log!("DEBUG: FrameAllocator: Considering frame: {:?} (offset {} in region {})", frame, self.current_frame_offset, self.current_region_idx);

                if self.is_frame_usable(frame.start_address()) {
                    //log!("DEBUG: FrameAllocator: ALLOCATED frame: {:?}", frame);
                    self.current_frame_offset += Size4KiB::SIZE as u64;
                    return Some(frame);
                } else {
                    //log!("DEBUG: FrameAllocator: SKIPPED unusable frame (framebuffer conflict): {:?}", frame);
                    self.current_frame_offset += Size4KiB::SIZE as u64;
                    continue;
                }
            } else {
                //log!("DEBUG: FrameAllocator: Region exhausted (index {}). Moving to next region.", self.current_region_idx);
                self.current_region_idx += 1;
                self.current_frame_offset = 0;
                continue;
            }
        }
    }
}