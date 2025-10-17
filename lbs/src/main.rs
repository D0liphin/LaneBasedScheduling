#![allow(unsafe_op_in_unsafe_fn, non_snake_case, non_camel_case_types)]
#![feature(likely_unlikely, thread_local)]

use libc::*;
use std::hint::{likely, unlikely};
use std::marker::PhantomData;
use std::mem;
use std::process;
mod util;
use util::*;
mod into_intptr;
use into_intptr::*;

/// Closures actually have a pure function part that is always the same. Since
/// we have to read from the buffer anyway, it makes sense to let the function
/// itself determine register allocation rather than trying to get smart with
/// making our own CC (at least for now).
type closure_fn_t = unsafe fn(*mut intptr_t);

const WORK_QUEUE_CAP: usize = 32;
const MAX_CLOSURE_CAPTURES: usize = 7;
const MAX_CLOSURE_SZ_INTPTRS: usize = MAX_CLOSURE_CAPTURES + 1;
const WORK_QUEUE_CAP_INTPTRS: usize = (WORK_QUEUE_CAP + 2) * MAX_CLOSURE_SZ_INTPTRS;

struct WorkQueue {
    queue: [intptr_t; WORK_QUEUE_CAP_INTPTRS],
    queue_hd: usize,
    queue_tl: usize,
    queue_len: usize,
}

const unsafe fn wq_new() -> WorkQueue {
    WorkQueue {
        queue: [0; WORK_QUEUE_CAP_INTPTRS],
        queue_hd: 0,
        queue_tl: 0,
        queue_len: 0,
    }
}

/// We store the `sz_intptrs` (max 128 -- 7 bits) in the 7 free bits at the
/// 'end' of the function pointer to preserve space... maybe not performance
/// optimal?
#[inline(always)]
unsafe fn make_closure_header(func: closure_fn_t, sz_intptrs: usize) -> intptr_t {
    let func: intptr_t = mem::transmute(func);
    (func & (uintptr_t::MAX >> 7) as intptr_t) | (sz_intptrs << 57) as intptr_t
}

#[inline(always)]
unsafe fn read_closure_header(hd: intptr_t) -> (closure_fn_t, usize) {
    let sz_intptrs = hd as usize >> 57;
    let func = (hd << 7) >> 7;
    (mem::transmute(func), sz_intptrs)
}

#[inline(always)]
unsafe fn wq_queue(wq: *mut WorkQueue) -> *mut intptr_t {
    addr!((*wq).queue) as *mut intptr_t
}

/// Return a contiguous region of memory to write to, as big as `sz_intptrs`.
#[inline(always)]
unsafe fn wq_enqueue_region(wq: *mut WorkQueue, sz_intptrs: usize) -> *mut intptr_t {
    if cfg!(debug_assertions) {
        assert!(sz_intptrs < MAX_CLOSURE_SZ_INTPTRS);
        assert!((*wq).queue_len < WORK_QUEUE_CAP);
    }
    (*wq).queue_len += 1;
    let (hd, tl) = ((*wq).queue_hd, (*wq).queue_tl);
    let new_tl = tl + sz_intptrs;
    if cfg!(debug_assertions) {
        // it should be clear that if the deque is structured as below, we
        // should not be allowed to overflow `hd`. We only need this assertion
        // in debug mode, since the user should keep track not to overflow the
        // WORK_QUEUE_CAP.
        //
        // [x][ ][ ][ ][ ][x][x][x][x]
        //     ^tl         ^hd
        assert_implies!(tl < hd, new_tl < hd);
    }
    if likely(new_tl < (*wq).queue.len()) {
        let ret = wq_queue(wq).add(tl);
        (*wq).queue_tl = new_tl;
        ret
    } else {
        *(*wq).queue.get_unchecked_mut(tl) = NULL as intptr_t;
        (*wq).queue_tl = sz_intptrs;
        wq_queue(wq)
    }
}

macro_rules! wq_enqueue {
    ($wq:expr, $func:expr, $($param:expr),* $(,)?) => {{
        let wq = $wq;
        const SZ_INTPTRS: usize = count_exprs!($($param),*) + 1;
        let region = wq_enqueue_region(wq, SZ_INTPTRS);
        *region = make_closure_header($func, SZ_INTPTRS);
        let mut offset = 1;
        $(
            *region.add(offset) = ($param).into_intptr();
            offset += 1;
        )*
        let _ = offset;
    }};
}

/// Pop from the front and call the function
#[inline(always)]
unsafe fn wq_dequeue_and_call(wq: *mut WorkQueue) {
    if cfg!(debug_assertions) {
        assert_ne!((*wq).queue_len, 0);
    }
    (*wq).queue_len -= 1;
    let (mut hd, tl) = ((*wq).queue_hd, (*wq).queue_tl);
    if cfg!(debug_assertions) {
        // this being false would indicate that the deque is empty
        assert_ne!(hd, tl);
    }
    let region = wq_queue(wq).add(hd);
    if unlikely(*region == NULL as intptr_t) {
        hd = 0;
    }
    let (func, sz_intptrs) = read_closure_header(*region);
    (*wq).queue_hd = hd + sz_intptrs;
    func(region);
}

struct Scheduler {
    queue_0: WorkQueue,
}

#[thread_local]
static mut SCHEDULER: Scheduler = unsafe { Scheduler { queue_0: wq_new() } };

#[repr(transparent)]
struct ClosureArgs<T: ClosureArgsInner> {
    pointer: *mut intptr_t,
    _phantom: PhantomData<T>,
}

impl<T: ClosureArgsInner> ClosureArgs<T> {
    #[inline(always)]
    unsafe fn read_closure_fn_args(&self) -> T {
        T::read_closure_fn_args(self.pointer)
    }
}

macro_rules! sched {
    (($after_lanes:expr => $lane:expr) ($($val:ident),*$(,)?) $body:block) => {{
        let tuple = ($($val),*,);
        unsafe fn infer_type<T: ClosureArgsInner>(_: T, func: unsafe fn(ClosureArgs<T>)) -> closure_fn_t {
            mem::transmute(func)
        }
        let func = infer_type(tuple, |arguments| {
            unsafe {
                let ($($val),*,) = arguments.read_closure_fn_args();
                $body
            }
        });
        if likely(SCHEDULER.queue_0.queue_len == WORK_QUEUE_CAP) {
            wq_dequeue_and_call(addr!(SCHEDULER.queue_0));
        }
        wq_enqueue!(addr!(SCHEDULER.queue_0), func, $($val),*);
    }};
}

#[inline(never)]
#[unsafe(no_mangle)]
unsafe fn uf_main() {
    let string = c"Hello, from the first one!".as_ptr();
    sched!((0 => 1) (string) {
        puts(string);
    });
    let string = c"Hello, from the second one!".as_ptr();
    sched!((0 => 1) (string) {
        puts(string);
    });
    for _ in 0..WORK_QUEUE_CAP {
        let zero: i64 = 0;
        sched!((0 => 1) (zero) { _ = zero; });
    }
}

/*
figure out the cache miss with a perf counter and then change the actual instructions as we go
using call all the time might not be a good idea, we might be better off jumping

we can actually do all the dependencies if our queue is at most 32 long which
seems to be long enough anyway we can check all the dependencies in one go like
that anyway
*/

// fn rdtsc

/*
we just use a single bitset, if we can't put something on tough shit we start
pulling from the head until that bit is low
*/

// let mut times = vec![];
// let mut rax_pointee = 5i32;
// let mut rax = addr_of_mut!(rax_pointee);
// let mut rdx = 56u64;
// let mut den = 4234124i64;
// let mut num = 21312314i64;
// for _ in 0..10000 {
//     let earlier = time::Instant::now();
//     for _ in 0..10000 {
//         asm!(
//             // "rdtsc", // remove/keep and measure total time
//             // "mov [{}], edx",
//             "cqo",
//             "nop","nop","nop","nop","nop","nop","nop","nop","nop","nop",
//             "nop","nop","nop","nop","nop","nop","nop","nop","nop","nop",
//             "xchg [{}], {}",
//             "nop","nop","nop","nop","nop","nop","nop","nop","nop","nop",
//             "nop","nop","nop","nop","nop","nop","nop","nop","nop","nop",
//             "nop","nop","nop","nop","nop","nop","nop","nop","nop","nop",
//             "nop","nop","nop","nop","nop","nop","nop","nop","nop","nop",
//             "nop","nop","nop","nop","nop","nop","nop","nop","nop","nop",
//             "nop","nop","nop","nop","nop","nop","nop","nop","nop","nop",
//             "nop","nop","nop","nop","nop","nop","nop","nop","nop","nop",
//             "nop","nop","nop","nop","nop","nop","nop","nop","nop","nop",
//             "nop","nop","nop","nop","nop","nop","nop","nop","nop","nop",
//             "nop","nop","nop","nop","nop","nop","nop","nop","nop","nop",
//             // den = in(reg) den,
//             // inlateout("rax") num => _,
//             // lateout("rdx") _,
//             in(reg) rax,
//             inout(reg) rdx,
//             options(nomem, nostack, preserves_flags)
//         );
//     }
//     times.push(earlier.elapsed().as_nanos());
// }
// times.sort();
// for i in (0usize..10000).step_by(500) {
//     let mut sum = 0;
//     for j in i..i + 500 {
//         sum += times[j];
//     }
//     print!("{} ", (sum as f64 / 500. / 100.).round() / 10.);
// }
// println!();

fn main() {
    unsafe {
        let pid = process::id() as u32;
        let prio = -20;
        setpriority(PRIO_PROCESS, pid, prio);
        uf_main();
    }
}
