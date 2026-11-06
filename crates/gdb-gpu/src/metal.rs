use crate::backend::{BfsResult, CpuFallbackBackend, GpuComputeBackend, PageRankResult};
use gdb_core::{GdbResult, VertexId};
use gdb_storage::ChunkedCsr;
#[cfg(target_os = "macos")]
use std::ffi::{c_char, c_void, CStr, CString};

/// Apple Silicon Metal Shading Language (MSL) Kernels for Graph Analytics.
/// Compiled at runtime into hardware `MTLComputePipelineState` objects on macOS.
pub const METAL_GRAPH_KERNELS: &str = r#"
#include <metal_stdlib>
using namespace metal;

// 1. Parallel Wavefront BFS & SSSP Kernel
kernel void parallel_bfs_step(
    device const uint32_t* offsets        [[buffer(0)]],
    device const uint32_t* target_indices [[buffer(1)]],
    device atomic_uint* distances         [[buffer(2)]],
    device atomic_uint* changed           [[buffer(3)]],
    constant uint32_t& current_depth      [[buffer(4)]],
    constant uint32_t& num_vertices       [[buffer(5)]],
    uint u [[thread_position_in_grid]]
) {
    if (u >= num_vertices) return;
    if (atomic_load_explicit(&distances[u], memory_order_relaxed) != current_depth) return;

    uint32_t start = offsets[u];
    uint32_t end = offsets[u + 1];
    uint32_t next_d = current_depth + 1u;

    for (uint32_t off = start; off < end; off++) {
        uint32_t v = target_indices[off];
        uint32_t expected = 0xFFFFFFFFu;
        if (atomic_compare_exchange_weak_explicit(
                &distances[v],
                &expected,
                next_d,
                memory_order_relaxed,
                memory_order_relaxed)) {
            atomic_store_explicit(changed, 1u, memory_order_relaxed);
        }
    }
}

// 2. Parallel Weakly Connected Components (WCC) Min-Label Propagation Kernel
kernel void parallel_wcc_step(
    device const uint32_t* offsets        [[buffer(0)]],
    device const uint32_t* target_indices [[buffer(1)]],
    device atomic_uint* comp              [[buffer(2)]],
    device atomic_uint* changed           [[buffer(3)]],
    constant uint32_t& num_vertices       [[buffer(4)]],
    uint u [[thread_position_in_grid]]
) {
    if (u >= num_vertices) return;
    uint32_t start = offsets[u];
    uint32_t end = offsets[u + 1];

    for (uint32_t off = start; off < end; off++) {
        uint32_t v = target_indices[off];
        uint32_t cu = atomic_load_explicit(&comp[u], memory_order_relaxed);
        uint32_t cv = atomic_load_explicit(&comp[v], memory_order_relaxed);
        if (cu < cv) {
            uint32_t prev = atomic_fetch_min_explicit(&comp[v], cu, memory_order_relaxed);
            if (cu < prev) {
                atomic_store_explicit(changed, 1u, memory_order_relaxed);
            }
        } else if (cv < cu) {
            uint32_t prev = atomic_fetch_min_explicit(&comp[u], cv, memory_order_relaxed);
            if (cv < prev) {
                atomic_store_explicit(changed, 1u, memory_order_relaxed);
            }
        }
    }
}

// 3. Parallel Undirected Sorted Adjacency Intersection for Triangle Counting
kernel void parallel_triangle_step(
    device const uint32_t* undir_offsets  [[buffer(0)]],
    device const uint32_t* undir_targets  [[buffer(1)]],
    device uint32_t* triangle_counts      [[buffer(2)]],
    constant uint32_t& num_vertices       [[buffer(3)]],
    uint u [[thread_position_in_grid]]
) {
    if (u >= num_vertices) return;
    uint32_t u_start = undir_offsets[u];
    uint32_t u_end = undir_offsets[u + 1];
    uint32_t deg = u_end - u_start;
    uint32_t count = 0u;

    for (uint32_t i = 0; i < deg; i++) {
        uint32_t v = undir_targets[u_start + i];
        uint32_t v_start = undir_offsets[v];
        uint32_t v_end = undir_offsets[v + 1];

        uint32_t p1 = u_start + i + 1u;
        uint32_t p2 = v_start;
        while (p1 < u_end && p2 < v_end) {
            uint32_t w1 = undir_targets[p1];
            uint32_t w2 = undir_targets[p2];
            if (w1 == w2) {
                count++;
                p1++;
                p2++;
            } else if (w1 < w2) {
                p1++;
            } else {
                p2++;
            }
        }
    }
    triangle_counts[u] = count;
}

// 4. Parallel In-Degree Histogram Kernel
kernel void parallel_degree_in(
    device const uint32_t* target_indices [[buffer(0)]],
    device atomic_uint* in_degrees        [[buffer(1)]],
    constant uint32_t& num_edges          [[buffer(2)]],
    uint e [[thread_position_in_grid]]
) {
    if (e >= num_edges) return;
    uint32_t v = target_indices[e];
    atomic_fetch_add_explicit(&in_degrees[v], 1u, memory_order_relaxed);
}

// 5. Parallel Pull-Based PageRank SpMV Kernel (Zero Atomic Contention)
kernel void parallel_pagerank_pull(
    device const uint32_t* in_offsets     [[buffer(0)]],
    device const uint32_t* in_sources     [[buffer(1)]],
    device const uint32_t* out_degrees    [[buffer(2)]],
    device const float* rank_in           [[buffer(3)]],
    device float* rank_out                [[buffer(4)]],
    constant float& base_rank             [[buffer(5)]],
    constant float& damping               [[buffer(6)]],
    constant uint32_t& num_vertices       [[buffer(7)]],
    uint v [[thread_position_in_grid]]
) {
    if (v >= num_vertices) return;
    uint32_t start = in_offsets[v];
    uint32_t end = in_offsets[v + 1];
    float sum = 0.0f;
    for (uint32_t off = start; off < end; off++) {
        uint32_t u = in_sources[off];
        uint32_t deg = out_degrees[u];
        if (deg > 0u) {
            sum += rank_in[u] / float(deg);
        }
    }
    rank_out[v] = base_rank + damping * sum;
}

// 6. Parallel Vector Similarity Kernel (Cosine, Dot Product, Euclidean L2)
kernel void parallel_vector_similarity(
    device const float* vectors           [[buffer(0)]],
    device const float* query             [[buffer(1)]],
    device float* scores                  [[buffer(2)]],
    constant uint32_t& num_vectors        [[buffer(3)]],
    constant uint32_t& dim                [[buffer(4)]],
    constant uint32_t& metric_mode        [[buffer(5)]],
    constant float& query_norm            [[buffer(6)]],
    uint i [[thread_position_in_grid]]
) {
    if (i >= num_vectors) return;
    uint32_t base = i * dim;
    if (metric_mode == 1u) {
        float dot = 0.0f;
        for (uint32_t d = 0; d < dim; d++) {
            dot += vectors[base + d] * query[d];
        }
        scores[i] = dot;
    } else if (metric_mode == 2u) {
        float dist_sq = 0.0f;
        for (uint32_t d = 0; d < dim; d++) {
            float diff = vectors[base + d] - query[d];
            dist_sq += diff * diff;
        }
        scores[i] = 1.0f / (1.0f + sqrt(dist_sq));
    } else {
        float dot = 0.0f;
        float v_sq = 0.0f;
        for (uint32_t d = 0; d < dim; d++) {
            float val = vectors[base + d];
            dot += val * query[d];
            v_sq += val * val;
        }
        float denom = sqrt(v_sq) * query_norm;
        scores[i] = (denom > 0.0f) ? (dot / denom) : 0.0f;
    }
}
"#;

#[cfg(target_os = "macos")]
#[link(name = "Metal", kind = "framework")]
#[link(name = "Foundation", kind = "framework")]
#[link(name = "objc", kind = "dylib")]
extern "C" {
    fn MTLCreateSystemDefaultDevice() -> *mut c_void;
    fn objc_getClass(name: *const c_char) -> *mut c_void;
    fn sel_registerName(name: *const c_char) -> *mut c_void;
    fn objc_msgSend();
    fn objc_autoreleasePoolPush() -> *mut c_void;
    fn objc_autoreleasePoolPop(pool: *mut c_void);
}

#[cfg(target_os = "macos")]
#[repr(C)]
#[derive(Copy, Clone, Debug)]
struct MTLSize {
    width: usize,
    height: usize,
    depth: usize,
}

#[cfg(target_os = "macos")]
struct MetalRuntime {
    device: *mut c_void,
    queue: *mut c_void,
    bfs_pipeline: *mut c_void,
    wcc_pipeline: *mut c_void,
    triangle_pipeline: *mut c_void,
    degree_pipeline: *mut c_void,
    pagerank_pipeline: *mut c_void,
    vector_pipeline: *mut c_void,
    sel_new_buf_bytes: *mut c_void,
    sel_new_buf_len: *mut c_void,
    sel_contents: *mut c_void,
    sel_release: *mut c_void,
    sel_cmd_buf: *mut c_void,
    sel_compute_enc: *mut c_void,
    sel_set_pipe: *mut c_void,
    sel_set_buf: *mut c_void,
    sel_set_bytes: *mut c_void,
    sel_dispatch: *mut c_void,
    sel_end_enc: *mut c_void,
    sel_commit: *mut c_void,
    sel_wait: *mut c_void,
}

#[cfg(target_os = "macos")]
unsafe impl Send for MetalRuntime {}
#[cfg(target_os = "macos")]
unsafe impl Sync for MetalRuntime {}

#[cfg(target_os = "macos")]
impl MetalRuntime {
    unsafe fn sel(name: &str) -> *mut c_void {
        let c = CString::new(name).unwrap();
        sel_registerName(c.as_ptr())
    }

    unsafe fn ns_string(s: &str) -> *mut c_void {
        let cls_c = CString::new("NSString").unwrap();
        let cls = objc_getClass(cls_c.as_ptr());
        let sel = Self::sel("stringWithUTF8String:");
        let c_str = CString::new(s).unwrap();
        let f: unsafe extern "C" fn(*mut c_void, *mut c_void, *const c_char) -> *mut c_void =
            std::mem::transmute(objc_msgSend as *const ());
        f(cls, sel, c_str.as_ptr())
    }

    unsafe fn init() -> Option<(Self, String)> {
        let pool = objc_autoreleasePoolPush();
        let device = MTLCreateSystemDefaultDevice();
        if device.is_null() {
            objc_autoreleasePoolPop(pool);
            return None;
        }

        // Read hardware GPU name: [[device name] UTF8String]
        let sel_name = Self::sel("name");
        let sel_utf8 = Self::sel("UTF8String");
        let f_obj0: unsafe extern "C" fn(*mut c_void, *mut c_void) -> *mut c_void =
            std::mem::transmute(objc_msgSend as *const ());
        let f_cstr: unsafe extern "C" fn(*mut c_void, *mut c_void) -> *const c_char =
            std::mem::transmute(objc_msgSend as *const ());
        let ns_dev_name = f_obj0(device, sel_name);
        let hw_name = if !ns_dev_name.is_null() {
            let ptr = f_cstr(ns_dev_name, sel_utf8);
            if !ptr.is_null() {
                CStr::from_ptr(ptr).to_string_lossy().into_owned()
            } else {
                "Apple Silicon GPU".to_string()
            }
        } else {
            "Apple Silicon GPU".to_string()
        };

        let sel_new_queue = Self::sel("newCommandQueue");
        let queue = f_obj0(device, sel_new_queue);
        if queue.is_null() {
            objc_autoreleasePoolPop(pool);
            return None;
        }

        // Compile METAL_GRAPH_KERNELS
        let src_ns = Self::ns_string(METAL_GRAPH_KERNELS);
        let sel_new_lib = Self::sel("newLibraryWithSource:options:error:");
        let f_lib: unsafe extern "C" fn(
            *mut c_void,
            *mut c_void,
            *mut c_void,
            *mut c_void,
            *mut *mut c_void,
        ) -> *mut c_void = std::mem::transmute(objc_msgSend as *const ());
        let mut err: *mut c_void = std::ptr::null_mut();
        let library = f_lib(
            device,
            sel_new_lib,
            src_ns,
            std::ptr::null_mut(),
            &mut err,
        );
        if library.is_null() {
            objc_autoreleasePoolPop(pool);
            return None;
        }

        let sel_new_fn = Self::sel("newFunctionWithName:");
        let sel_new_pipe = Self::sel("newComputePipelineStateWithFunction:error:");
        let sel_release = Self::sel("release");
        let f_fn: unsafe extern "C" fn(*mut c_void, *mut c_void, *mut c_void) -> *mut c_void =
            std::mem::transmute(objc_msgSend as *const ());
        let f_pipe: unsafe extern "C" fn(
            *mut c_void,
            *mut c_void,
            *mut c_void,
            *mut *mut c_void,
        ) -> *mut c_void = std::mem::transmute(objc_msgSend as *const ());
        let f_void0: unsafe extern "C" fn(*mut c_void, *mut c_void) =
            std::mem::transmute(objc_msgSend as *const ());

        let compile_kernel = |fn_name: &str| -> Option<*mut c_void> {
            let fn_ns = Self::ns_string(fn_name);
            let func = f_fn(library, sel_new_fn, fn_ns);
            if func.is_null() {
                return None;
            }
            let mut pipe_err: *mut c_void = std::ptr::null_mut();
            let pipe = f_pipe(device, sel_new_pipe, func, &mut pipe_err);
            f_void0(func, sel_release);
            if pipe.is_null() {
                None
            } else {
                Some(pipe)
            }
        };

        let bfs_pipeline = compile_kernel("parallel_bfs_step")?;
        let wcc_pipeline = compile_kernel("parallel_wcc_step")?;
        let triangle_pipeline = compile_kernel("parallel_triangle_step")?;
        let degree_pipeline = compile_kernel("parallel_degree_in")?;
        let pagerank_pipeline = compile_kernel("parallel_pagerank_pull")?;
        let vector_pipeline = compile_kernel("parallel_vector_similarity")?;

        f_void0(library, sel_release);
        objc_autoreleasePoolPop(pool);

        let rt = Self {
            device,
            queue,
            bfs_pipeline,
            wcc_pipeline,
            triangle_pipeline,
            degree_pipeline,
            pagerank_pipeline,
            vector_pipeline,
            sel_new_buf_bytes: Self::sel("newBufferWithBytes:length:options:"),
            sel_new_buf_len: Self::sel("newBufferWithLength:options:"),
            sel_contents: Self::sel("contents"),
            sel_release,
            sel_cmd_buf: Self::sel("commandBuffer"),
            sel_compute_enc: Self::sel("computeCommandEncoder"),
            sel_set_pipe: Self::sel("setComputePipelineState:"),
            sel_set_buf: Self::sel("setBuffer:offset:atIndex:"),
            sel_set_bytes: Self::sel("setBytes:length:atIndex:"),
            sel_dispatch: Self::sel("dispatchThreadgroups:threadsPerThreadgroup:"),
            sel_end_enc: Self::sel("endEncoding"),
            sel_commit: Self::sel("commit"),
            sel_wait: Self::sel("waitUntilCompleted"),
        };

        Some((rt, hw_name))
    }

    #[inline]
    unsafe fn alloc_shared_slice<T: Copy>(&self, slice: &[T]) -> *mut c_void {
        let byte_len = (slice.len() * std::mem::size_of::<T>()).max(4);
        let f_buf: unsafe extern "C" fn(
            *mut c_void,
            *mut c_void,
            *const c_void,
            usize,
            usize,
        ) -> *mut c_void = std::mem::transmute(objc_msgSend as *const ());
        let f_len: unsafe extern "C" fn(*mut c_void, *mut c_void, usize, usize) -> *mut c_void =
            std::mem::transmute(objc_msgSend as *const ());

        if slice.is_empty() {
            f_len(self.device, self.sel_new_buf_len, byte_len, 0)
        } else {
            f_buf(
                self.device,
                self.sel_new_buf_bytes,
                slice.as_ptr() as *const c_void,
                byte_len,
                0, // MTLResourceStorageModeShared (UMA Zero-Copy)
            )
        }
    }

    #[inline]
    unsafe fn buf_contents<T>(&self, buf: *mut c_void) -> *mut T {
        let f_obj0: unsafe extern "C" fn(*mut c_void, *mut c_void) -> *mut c_void =
            std::mem::transmute(objc_msgSend as *const ());
        f_obj0(buf, self.sel_contents) as *mut T
    }

    #[inline]
    unsafe fn release_buf(&self, buf: *mut c_void) {
        if !buf.is_null() {
            let f_void0: unsafe extern "C" fn(*mut c_void, *mut c_void) =
                std::mem::transmute(objc_msgSend as *const ());
            f_void0(buf, self.sel_release);
        }
    }

    unsafe fn dispatch_kernel(
        &self,
        pipeline: *mut c_void,
        buffers: &[(*mut c_void, usize)],
        inline_constants: &[(*const c_void, usize, usize)],
        total_threads: usize,
    ) {
        if total_threads == 0 {
            return;
        }
        let pool = objc_autoreleasePoolPush();
        let f_obj0: unsafe extern "C" fn(*mut c_void, *mut c_void) -> *mut c_void =
            std::mem::transmute(objc_msgSend as *const ());
        let f_void0: unsafe extern "C" fn(*mut c_void, *mut c_void) =
            std::mem::transmute(objc_msgSend as *const ());
        let f_set_pipe: unsafe extern "C" fn(*mut c_void, *mut c_void, *mut c_void) =
            std::mem::transmute(objc_msgSend as *const ());
        let f_set_buf: unsafe extern "C" fn(*mut c_void, *mut c_void, *mut c_void, usize, usize) =
            std::mem::transmute(objc_msgSend as *const ());
        let f_set_bytes: unsafe extern "C" fn(
            *mut c_void,
            *mut c_void,
            *const c_void,
            usize,
            usize,
        ) = std::mem::transmute(objc_msgSend as *const ());
        let f_dispatch: unsafe extern "C" fn(*mut c_void, *mut c_void, MTLSize, MTLSize) =
            std::mem::transmute(objc_msgSend as *const ());

        let cmd_buf = f_obj0(self.queue, self.sel_cmd_buf);
        let encoder = f_obj0(cmd_buf, self.sel_compute_enc);

        f_set_pipe(encoder, self.sel_set_pipe, pipeline);
        for &(buf, idx) in buffers {
            f_set_buf(encoder, self.sel_set_buf, buf, 0, idx);
        }
        for &(ptr, len, idx) in inline_constants {
            f_set_bytes(encoder, self.sel_set_bytes, ptr, len, idx);
        }

        let tg_width = 256usize.min(total_threads.max(1));
        let num_groups = (total_threads + tg_width - 1) / tg_width;
        let groups = MTLSize {
            width: num_groups.max(1),
            height: 1,
            depth: 1,
        };
        let threads = MTLSize {
            width: tg_width,
            height: 1,
            depth: 1,
        };

        f_dispatch(encoder, self.sel_dispatch, groups, threads);
        f_void0(encoder, self.sel_end_enc);
        f_void0(cmd_buf, self.sel_commit);
        f_void0(cmd_buf, self.sel_wait);
        objc_autoreleasePoolPop(pool);
    }
}

pub struct MetalComputeBackend {
    #[allow(dead_code)]
    device_name: String,
    #[cfg(target_os = "macos")]
    runtime: Option<MetalRuntime>,
}

impl Default for MetalComputeBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl MetalComputeBackend {
    pub fn new() -> Self {
        Self::with_device(0)
    }

    pub fn with_device(device_id: u32) -> Self {
        #[cfg(target_os = "macos")]
        {
            if let Some((rt, hw_name)) = unsafe { MetalRuntime::init() } {
                let name = format!("{} (Metal UMA Zero-Copy #{})", hw_name, device_id);
                tracing::info!("Initialized hardware GPU backend: {}", name);
                return Self {
                    device_name: name,
                    runtime: Some(rt),
                };
            }
        }
        Self {
            device_name: format!("Apple Silicon Metal Device #{} (UMA Zero-Copy)", device_id),
            #[cfg(target_os = "macos")]
            runtime: None,
        }
    }

    #[cfg(target_os = "macos")]
    fn run_gpu_bfs_or_sssp(
        &self,
        rt: &MetalRuntime,
        csr: &ChunkedCsr,
        start_vid: VertexId,
        max_depth: u32,
        include_unreachable: bool,
    ) -> GdbResult<Vec<(VertexId, u32)>> {
        let num_v = csr.vertex_count();
        if num_v == 0 {
            return Ok(Vec::new());
        }

        let Some(&start_idx) = csr.vertex_map.get(&start_vid.as_u64()) else {
            if include_unreachable {
                return Ok(csr
                    .reverse_map
                    .iter()
                    .map(|&raw| (VertexId(raw), u32::MAX))
                    .collect());
            }
            return Ok(Vec::new());
        };

        let mut init_distances = vec![u32::MAX; num_v];
        init_distances[start_idx as usize] = 0;

        unsafe {
            let buf_offsets = rt.alloc_shared_slice(&csr.offsets);
            let buf_targets = rt.alloc_shared_slice(&csr.target_indices);
            let buf_distances = rt.alloc_shared_slice(&init_distances);
            let buf_changed = rt.alloc_shared_slice(&[0u32]);

            let changed_ptr = rt.buf_contents::<u32>(buf_changed);
            let num_v_u32 = num_v as u32;

            let gpu_steps = max_depth.min(16);
            let mut finished = false;
            for depth in 0..gpu_steps {
                *changed_ptr = 0;
                rt.dispatch_kernel(
                    rt.bfs_pipeline,
                    &[
                        (buf_offsets, 0),
                        (buf_targets, 1),
                        (buf_distances, 2),
                        (buf_changed, 3),
                    ],
                    &[
                        (
                            &depth as *const u32 as *const c_void,
                            std::mem::size_of::<u32>(),
                            4,
                        ),
                        (
                            &num_v_u32 as *const u32 as *const c_void,
                            std::mem::size_of::<u32>(),
                            5,
                        ),
                    ],
                    num_v,
                );
                if *changed_ptr == 0 {
                    finished = true;
                    break;
                }
            }

            let dist_slice = std::slice::from_raw_parts_mut(rt.buf_contents::<u32>(buf_distances), num_v);
            if !finished && max_depth > gpu_steps {
                // Hybrid UMA tail completion for high-diameter graphs (depth > 16)
                let mut queue = std::collections::VecDeque::new();
                for u in 0..num_v {
                    if dist_slice[u] == gpu_steps {
                        queue.push_back(u);
                    }
                }
                while let Some(u) = queue.pop_front() {
                    let cur_d = dist_slice[u];
                    if cur_d >= max_depth {
                        continue;
                    }
                    let start = csr.offsets[u] as usize;
                    let end = (csr.offsets[u + 1] as usize).min(csr.target_indices.len());
                    for off in start..end {
                        let v = csr.target_indices[off] as usize;
                        if dist_slice[v] == u32::MAX {
                            dist_slice[v] = cur_d + 1;
                            queue.push_back(v);
                        }
                    }
                }
            }
            let mut results = Vec::with_capacity(num_v);
            for (idx, &d) in dist_slice.iter().enumerate() {
                if include_unreachable || d != u32::MAX {
                    results.push((VertexId(csr.reverse_map[idx]), d));
                }
            }

            rt.release_buf(buf_offsets);
            rt.release_buf(buf_targets);
            rt.release_buf(buf_distances);
            rt.release_buf(buf_changed);

            Ok(results)
        }
    }
}

impl GpuComputeBackend for MetalComputeBackend {
    fn name(&self) -> &'static str {
        "Apple Metal Compute (UMA Zero-Copy)"
    }

    fn parallel_bfs(
        &self,
        csr: &ChunkedCsr,
        start_vid: VertexId,
        max_depth: u32,
    ) -> GdbResult<BfsResult> {
        #[cfg(target_os = "macos")]
        if let Some(ref rt) = self.runtime {
            if !csr.target_indices.is_empty() {
                return self.run_gpu_bfs_or_sssp(rt, csr, start_vid, max_depth, false);
            }
        }
        CpuFallbackBackend.parallel_bfs(csr, start_vid, max_depth)
    }

    fn sssp(&self, csr: &ChunkedCsr, source: VertexId) -> GdbResult<Vec<(VertexId, u32)>> {
        #[cfg(target_os = "macos")]
        if let Some(ref rt) = self.runtime {
            if !csr.target_indices.is_empty() {
                let max_d = csr.vertex_count() as u32;
                return self.run_gpu_bfs_or_sssp(rt, csr, source, max_d, true);
            }
        }
        CpuFallbackBackend.sssp(csr, source)
    }

    fn pagerank(
        &self,
        csr: &ChunkedCsr,
        damping: f32,
        iterations: usize,
    ) -> GdbResult<PageRankResult> {
        #[cfg(target_os = "macos")]
        if let Some(ref rt) = self.runtime {
            let num_v = csr.vertex_count();
            if num_v > 0 && !csr.target_indices.is_empty() {
                // Build incoming CSR transpose for lock-free pull-based GPU SpMV
                let num_e = csr.target_indices.len();
                let mut out_degrees = vec![0u32; num_v];
                let mut in_counts = vec![0u32; num_v + 1];
                for u in 0..num_v {
                    let start = csr.offsets[u] as usize;
                    let end = (csr.offsets[u + 1] as usize).min(num_e);
                    out_degrees[u] = end.saturating_sub(start) as u32;
                    for off in start..end {
                        let v = csr.target_indices[off] as usize;
                        in_counts[v + 1] += 1;
                    }
                }
                for v in 0..num_v {
                    in_counts[v + 1] += in_counts[v];
                }
                let mut in_sources = vec![0u32; num_e];
                let mut cur = in_counts.clone();
                for u in 0..num_v {
                    let start = csr.offsets[u] as usize;
                    let end = (csr.offsets[u + 1] as usize).min(num_e);
                    for off in start..end {
                        let v = csr.target_indices[off] as usize;
                        let pos = cur[v] as usize;
                        cur[v] += 1;
                        in_sources[pos] = u as u32;
                    }
                }

                let n = num_v as f32;
                let init_rank = vec![1.0f32 / n; num_v];
                unsafe {
                    let buf_in_offsets = rt.alloc_shared_slice(&in_counts);
                    let buf_in_sources = rt.alloc_shared_slice(&in_sources);
                    let buf_out_deg = rt.alloc_shared_slice(&out_degrees);
                    let mut buf_rank_a = rt.alloc_shared_slice(&init_rank);
                    let mut buf_rank_b = rt.alloc_shared_slice(&vec![0.0f32; num_v]);
                    let num_v_u32 = num_v as u32;

                    for _ in 0..iterations {
                        let rank_in_slice =
                            std::slice::from_raw_parts(rt.buf_contents::<f32>(buf_rank_a), num_v);
                        let mut dangling_sum = 0.0f32;
                        for u in 0..num_v {
                            if out_degrees[u] == 0 {
                                dangling_sum += rank_in_slice[u];
                            }
                        }
                        let base_rank = (1.0 - damping) / n + (damping * dangling_sum) / n;

                        rt.dispatch_kernel(
                            rt.pagerank_pipeline,
                            &[
                                (buf_in_offsets, 0),
                                (buf_in_sources, 1),
                                (buf_out_deg, 2),
                                (buf_rank_a, 3),
                                (buf_rank_b, 4),
                            ],
                            &[
                                (
                                    &base_rank as *const f32 as *const c_void,
                                    std::mem::size_of::<f32>(),
                                    5,
                                ),
                                (
                                    &damping as *const f32 as *const c_void,
                                    std::mem::size_of::<f32>(),
                                    6,
                                ),
                                (
                                    &num_v_u32 as *const u32 as *const c_void,
                                    std::mem::size_of::<u32>(),
                                    7,
                                ),
                            ],
                            num_v,
                        );

                        std::mem::swap(&mut buf_rank_a, &mut buf_rank_b);
                    }

                    let final_slice =
                        std::slice::from_raw_parts(rt.buf_contents::<f32>(buf_rank_a), num_v);
                    let mut res = Vec::with_capacity(num_v);
                    for i in 0..num_v {
                        res.push((VertexId(csr.reverse_map[i]), final_slice[i]));
                    }

                    rt.release_buf(buf_in_offsets);
                    rt.release_buf(buf_in_sources);
                    rt.release_buf(buf_out_deg);
                    rt.release_buf(buf_rank_a);
                    rt.release_buf(buf_rank_b);

                    return Ok(res);
                }
            }
        }
        CpuFallbackBackend.pagerank(csr, damping, iterations)
    }

    fn wcc(&self, csr: &ChunkedCsr) -> GdbResult<Vec<(VertexId, u64)>> {
        #[cfg(target_os = "macos")]
        if let Some(ref rt) = self.runtime {
            let num_v = csr.vertex_count();
            if num_v > 0 && !csr.target_indices.is_empty() {
                let init_comp: Vec<u32> = (0..num_v as u32).collect();
                unsafe {
                    let buf_offsets = rt.alloc_shared_slice(&csr.offsets);
                    let buf_targets = rt.alloc_shared_slice(&csr.target_indices);
                    let buf_comp = rt.alloc_shared_slice(&init_comp);
                    let buf_changed = rt.alloc_shared_slice(&[0u32]);

                    let changed_ptr = rt.buf_contents::<u32>(buf_changed);
                    let comp_ptr = rt.buf_contents::<u32>(buf_comp);
                    let num_v_u32 = num_v as u32;

                    let mut converged = false;
                    for _ in 0..4 {
                        *changed_ptr = 0;
                        rt.dispatch_kernel(
                            rt.wcc_pipeline,
                            &[
                                (buf_offsets, 0),
                                (buf_targets, 1),
                                (buf_comp, 2),
                                (buf_changed, 3),
                            ],
                            &[(
                                &num_v_u32 as *const u32 as *const c_void,
                                std::mem::size_of::<u32>(),
                                4,
                            )],
                            num_v,
                        );
                        if *changed_ptr == 0 {
                            converged = true;
                            break;
                        }
                    }

                    let comp_slice = std::slice::from_raw_parts_mut(comp_ptr, num_v);
                    if !converged {
                        // Single-pass Union-Find completion in UMA shared memory for high-diameter graphs
                        fn find_root(parent: &mut [u32], mut i: usize) -> usize {
                            while parent[i] as usize != i {
                                let p = parent[i] as usize;
                                parent[i] = parent[p];
                                i = p;
                            }
                            i
                        }
                        for u in 0..num_v {
                            let start = csr.offsets[u] as usize;
                            let end = (csr.offsets[u + 1] as usize).min(csr.target_indices.len());
                            for off in start..end {
                                let v = csr.target_indices[off] as usize;
                                let ru = find_root(comp_slice, u);
                                let rv = find_root(comp_slice, v);
                                if ru != rv {
                                    if ru < rv {
                                        comp_slice[rv] = ru as u32;
                                    } else {
                                        comp_slice[ru] = rv as u32;
                                    }
                                }
                            }
                        }
                        for u in 0..num_v {
                            let r = find_root(comp_slice, u);
                            comp_slice[u] = r as u32;
                        }
                    }
                    let mut res = Vec::with_capacity(num_v);
                    for i in 0..num_v {
                        let root_idx = comp_slice[i] as usize;
                        res.push((
                            VertexId(csr.reverse_map[i]),
                            csr.reverse_map[root_idx],
                        ));
                    }

                    rt.release_buf(buf_offsets);
                    rt.release_buf(buf_targets);
                    rt.release_buf(buf_comp);
                    rt.release_buf(buf_changed);

                    return Ok(res);
                }
            }
        }
        CpuFallbackBackend.wcc(csr)
    }

    fn louvain(&self, csr: &ChunkedCsr, max_iter: usize) -> GdbResult<Vec<(VertexId, u64)>> {
        CpuFallbackBackend.louvain(csr, max_iter)
    }

    fn triangle_count(&self, csr: &ChunkedCsr) -> GdbResult<Vec<(VertexId, u64)>> {
        #[cfg(target_os = "macos")]
        if let Some(ref rt) = self.runtime {
            let num_v = csr.vertex_count();
            if num_v > 0 {
                let adj = csr.build_undirected_adj();
                let mut undir_offsets = Vec::with_capacity(num_v + 1);
                undir_offsets.push(0u32);
                let mut total_undir = 0u32;
                for list in &adj {
                    total_undir += list.len() as u32;
                    undir_offsets.push(total_undir);
                }
                let mut undir_targets = Vec::with_capacity(total_undir as usize);
                for list in &adj {
                    undir_targets.extend_from_slice(list);
                }

                unsafe {
                    let buf_offsets = rt.alloc_shared_slice(&undir_offsets);
                    let buf_targets = rt.alloc_shared_slice(&undir_targets);
                    let buf_counts = rt.alloc_shared_slice(&vec![0u32; num_v]);
                    let num_v_u32 = num_v as u32;

                    rt.dispatch_kernel(
                        rt.triangle_pipeline,
                        &[(buf_offsets, 0), (buf_targets, 1), (buf_counts, 2)],
                        &[(
                            &num_v_u32 as *const u32 as *const c_void,
                            std::mem::size_of::<u32>(),
                            3,
                        )],
                        num_v,
                    );

                    let counts_slice =
                        std::slice::from_raw_parts(rt.buf_contents::<u32>(buf_counts), num_v);
                    let mut res = Vec::with_capacity(num_v);
                    for i in 0..num_v {
                        res.push((VertexId(csr.reverse_map[i]), counts_slice[i] as u64));
                    }

                    rt.release_buf(buf_offsets);
                    rt.release_buf(buf_targets);
                    rt.release_buf(buf_counts);

                    return Ok(res);
                }
            }
        }
        CpuFallbackBackend.triangle_count(csr)
    }

    fn degree(&self, csr: &ChunkedCsr) -> GdbResult<Vec<(VertexId, u32, u32, u32)>> {
        #[cfg(target_os = "macos")]
        if let Some(ref rt) = self.runtime {
            let num_v = csr.vertex_count();
            let num_e = csr.target_indices.len();
            if num_v > 0 && num_e > 0 {
                unsafe {
                    let buf_targets = rt.alloc_shared_slice(&csr.target_indices);
                    let buf_in_deg = rt.alloc_shared_slice(&vec![0u32; num_v]);
                    let num_e_u32 = num_e as u32;

                    rt.dispatch_kernel(
                        rt.degree_pipeline,
                        &[(buf_targets, 0), (buf_in_deg, 1)],
                        &[(
                            &num_e_u32 as *const u32 as *const c_void,
                            std::mem::size_of::<u32>(),
                            2,
                        )],
                        num_e,
                    );

                    let in_slice =
                        std::slice::from_raw_parts(rt.buf_contents::<u32>(buf_in_deg), num_v);
                    let mut res = Vec::with_capacity(num_v);
                    for u in 0..num_v {
                        let start = csr.offsets[u] as usize;
                        let end = (csr.offsets[u + 1] as usize).min(num_e);
                        let out_d = end.saturating_sub(start) as u32;
                        let in_d = in_slice[u];
                        res.push((VertexId(csr.reverse_map[u]), in_d, out_d, in_d + out_d));
                    }

                    rt.release_buf(buf_targets);
                    rt.release_buf(buf_in_deg);

                    return Ok(res);
                }
            }
        }
        CpuFallbackBackend.degree(csr)
    }

    fn vector_similarity(
        &self,
        vectors: &[f32],
        dim: usize,
        query: &[f32],
        k: usize,
        metric: &str,
    ) -> GdbResult<Vec<(usize, f32)>> {
        if dim == 0 || query.len() != dim || vectors.is_empty() {
            return Ok(Vec::new());
        }

        #[cfg(target_os = "macos")]
        if let Some(ref rt) = self.runtime {
            let num_vectors = vectors.len() / dim;
            if num_vectors > 0 {
                let metric_mode: u32 = match metric.to_lowercase().as_str() {
                    "dot" | "dot_product" => 1,
                    "l2" | "euclidean" => 2,
                    _ => 0,
                };
                let query_norm: f32 = if metric_mode == 0 {
                    query.iter().map(|x| x * x).sum::<f32>().sqrt()
                } else {
                    1.0
                };

                unsafe {
                    let buf_vectors = rt.alloc_shared_slice(vectors);
                    let buf_query = rt.alloc_shared_slice(query);
                    let buf_scores = rt.alloc_shared_slice(&vec![0.0f32; num_vectors]);
                    let num_v_u32 = num_vectors as u32;
                    let dim_u32 = dim as u32;

                    rt.dispatch_kernel(
                        rt.vector_pipeline,
                        &[(buf_vectors, 0), (buf_query, 1), (buf_scores, 2)],
                        &[
                            (
                                &num_v_u32 as *const u32 as *const c_void,
                                std::mem::size_of::<u32>(),
                                3,
                            ),
                            (
                                &dim_u32 as *const u32 as *const c_void,
                                std::mem::size_of::<u32>(),
                                4,
                            ),
                            (
                                &metric_mode as *const u32 as *const c_void,
                                std::mem::size_of::<u32>(),
                                5,
                            ),
                            (
                                &query_norm as *const f32 as *const c_void,
                                std::mem::size_of::<f32>(),
                                6,
                            ),
                        ],
                        num_vectors,
                    );

                    let scores_slice = std::slice::from_raw_parts(
                        rt.buf_contents::<f32>(buf_scores),
                        num_vectors,
                    );
                    let mut scored: Vec<(usize, f32)> = scores_slice
                        .iter()
                        .enumerate()
                        .map(|(i, &s)| (i, s))
                        .collect();

                    rt.release_buf(buf_vectors);
                    rt.release_buf(buf_query);
                    rt.release_buf(buf_scores);

                    scored.sort_unstable_by(|a, b| {
                        b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal)
                    });
                    scored.truncate(k);
                    return Ok(scored);
                }
            }
        }

        CpuFallbackBackend.vector_similarity(vectors, dim, query, k, metric)
    }
}
