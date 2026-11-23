//! Compatibility shims for MSVC STL vectorized-algorithm symbols.
//!
//! The prebuilt Windows ONNX Runtime static library downloaded by `ort-sys`
//! (pyke.io `ms@1.28.0`, WebGPU build) was compiled against an MSVC STL whose
//! manually-vectorized `<algorithm>` helpers use the *typed* names:
//! `__std_max_element_8i`, `__std_min_element_4i`, `__std_rotate`, ...
//!
//! MSVC 14.44 (VS 2022 17.14) renamed/removed those helpers in `msvcprt.lib`
//! (it now ships `__std_max_element_8(first, last, signed)`, etc.). Linking the
//! ORT library therefore fails with `LNK2019: unresolved external symbol
//! __std_*`. There is no linker flag or library to install that restores the
//! old names — they simply are not in the local CRT.
//!
//! This module supplies the old symbols with straightforward *scalar*
//! implementations of the same standard algorithms. ORT only uses these as
//! optional SIMD fast paths, so correctness is unchanged and the performance
//! impact is limited to the few algorithm calls inside ORT that would have
//! been vectorized. All functions here are `extern "system"` (stdcall naming
//! on x86, plain C on x64) and are only compiled for MSVC x86-64, the target
//! of the Windows prebuilt ORT binary.
//!
//! Remove this module once ORT ships a binary built against a matching STL.

#![allow(non_snake_case, clippy::not_unsafe_ptr_arg_deref)]

use std::ffi::c_void;

/// Return type of `__std_minmax_element_*`: `{ _Min, _Max }`, two pointers.
/// In the x64 Microsoft ABI a 16-byte POD is returned in RAX:RDX, which is
/// exactly what `extern "system"` with this `#[repr(C)]` struct produces.
#[repr(C)]
pub struct MinMaxElement {
    min: *const c_void,
    max: *const c_void,
}

/// `std::max_element` / `std::min_element` for a single trivially-copyable
/// type. `want_max` selects the direction; ties keep the first element, as in
/// the standard algorithms.
macro_rules! extrema {
    ($name:ident, $ty:ty, $want_max:expr) => {
        #[unsafe(no_mangle)]
        pub extern "system" fn $name(first: *const c_void, last: *const c_void) -> *const c_void {
            unsafe {
                let first_addr = first as usize;
                let last_addr = last as usize;
                if first.is_null() || last_addr <= first_addr {
                    return last;
                }
                let len = (last_addr - first_addr) / std::mem::size_of::<$ty>();
                let p = first as *const $ty;
                let mut best = p;
                for i in 1..len {
                    let cur = p.add(i);
                    if ($want_max && *cur > *best) || (!$want_max && *cur < *best) {
                        best = cur;
                    }
                }
                best.cast()
            }
        }
    };
}

macro_rules! minmax_extrema {
    ($name:ident, $ty:ty) => {
        #[unsafe(no_mangle)]
        pub extern "system" fn $name(first: *const c_void, last: *const c_void) -> MinMaxElement {
            unsafe {
                let first_addr = first as usize;
                let last_addr = last as usize;
                if first.is_null() || last_addr <= first_addr {
                    return MinMaxElement { min: last, max: last };
                }
                let len = (last_addr - first_addr) / std::mem::size_of::<$ty>();
                let p = first as *const $ty;
                let (mut mn, mut mx) = (p, p);
                for i in 1..len {
                    let cur = p.add(i);
                    if *cur < *mn {
                        mn = cur;
                    }
                    // `<=` makes the max the *last* of equal elements, as in
                    // the C++ standard's minmax_element.
                    if *mx <= *cur {
                        mx = cur;
                    }
                }
                MinMaxElement { min: mn.cast(), max: mx.cast() }
            }
        }
    };
}

/// `std::unique`: compact consecutive duplicates in place, return the new
/// logical end.
macro_rules! unique {
    ($name:ident, $ty:ty) => {
        #[unsafe(no_mangle)]
        pub extern "system" fn $name(first: *mut c_void, last: *mut c_void) -> *mut c_void {
            unsafe {
                let first_addr = first as usize;
                let last_addr = last as usize;
                if first.is_null() || last_addr <= first_addr {
                    return last;
                }
                let len = (last_addr - first_addr) / std::mem::size_of::<$ty>();
                let p = first as *mut $ty;
                let mut out = p;
                for i in 1..len {
                    let cur = p.add(i);
                    if *cur != *out {
                        out = out.add(1);
                        *out = *cur;
                    }
                }
                out.add(1).cast()
            }
        }
    };
}

/// `std::search(first1, last1, first2, first2 + count2)`.
macro_rules! search {
    ($name:ident, $ty:ty) => {
        #[unsafe(no_mangle)]
        pub extern "system" fn $name(
            first1: *const c_void,
            last1: *const c_void,
            first2: *const c_void,
            count2: usize,
        ) -> *const c_void {
            unsafe {
                let first_addr = first1 as usize;
                let last_addr = last1 as usize;
                if first1.is_null() || first2.is_null() || count2 == 0 {
                    return first1;
                }
                let len = (last_addr - first_addr) / std::mem::size_of::<$ty>();
                if count2 > len {
                    return last1;
                }
                let a = first1 as *const $ty;
                let b = first2 as *const $ty;
                for i in 0..=(len - count2) {
                    let p = a.add(i);
                    if *p == *b && (1..count2).all(|j| *p.add(j) == *b.add(j)) {
                        return p.cast();
                    }
                }
                last1
            }
        }
    };
}

/// `std::is_sorted_until(first, last, greater ? greater<> : less<>)`,
/// returning the first out-of-order element (or `last`).
macro_rules! sorted_until {
    ($name:ident, $ty:ty) => {
        #[unsafe(no_mangle)]
        pub extern "system" fn $name(
            first: *const c_void,
            last: *const c_void,
            greater: bool,
        ) -> *const c_void {
            unsafe {
                let first_addr = first as usize;
                let last_addr = last as usize;
                if first.is_null() || last_addr <= first_addr {
                    return last;
                }
                let len = (last_addr - first_addr) / std::mem::size_of::<$ty>();
                let p = first as *const $ty;
                for i in 1..len {
                    let cur = *p.add(i);
                    let prev = *p.add(i - 1);
                    if if greater { cur > prev } else { cur < prev } {
                        return p.add(i).cast();
                    }
                }
                last
            }
        }
    };
}

/// `basic_string::find_first_not_of` vectorized helper for a trivial element
/// type. Lengths are element counts.
macro_rules! find_first_not_of {
    ($name:ident, $ty:ty) => {
        #[unsafe(no_mangle)]
        pub extern "system" fn $name(
            haystack: *const c_void,
            haystack_len: usize,
            needle: *const c_void,
            needle_len: usize,
        ) -> usize {
            unsafe {
                let hay = haystack as *const $ty;
                let needles = needle as *const $ty;
                for i in 0..haystack_len {
                    let c = *hay.add(i);
                    let found = (0..needle_len).any(|j| *needles.add(j) == c);
                    if !found {
                        return i;
                    }
                }
                haystack_len
            }
        }
    };
}

/// `std::replace_copy(first, last, dest, old_val, new_val)`.
macro_rules! replace_copy {
    ($name:ident, $ty:ty) => {
        #[unsafe(no_mangle)]
        pub extern "system" fn $name(
            first: *const c_void,
            last: *const c_void,
            dest: *mut c_void,
            old_val: $ty,
            new_val: $ty,
        ) {
            unsafe {
                let first_addr = first as usize;
                let last_addr = last as usize;
                if first.is_null() || dest.is_null() || last_addr <= first_addr {
                    return;
                }
                let len = (last_addr - first_addr) / std::mem::size_of::<$ty>();
                let src = first as *const $ty;
                let dst = dest as *mut $ty;
                for i in 0..len {
                    let v = *src.add(i);
                    *dst.add(i) = if v == old_val { new_val } else { v };
                }
            }
        }
    };
}

/// `std::rotate(first, mid, last)` on a byte range. `mid` and `last` are
/// element-aligned, so rotating the byte sequence by `(mid - first)` bytes is
/// equivalent to rotating whole elements. Uses the gcd cycle algorithm.
#[unsafe(no_mangle)]
pub extern "system" fn __std_rotate(first: *mut c_void, mid: *mut c_void, last: *mut c_void) {
    unsafe {
        if first.is_null() || mid.is_null() || last.is_null() {
            return;
        }
        let first_addr = first as usize;
        let total = (last as usize).wrapping_sub(first_addr);
        let shift = (mid as usize).wrapping_sub(first_addr);
        if total == 0 || shift == 0 || shift >= total {
            return;
        }
        let bytes = first as *mut u8;
        let gcd = |mut a: usize, mut b: usize| {
            while b != 0 {
                (a, b) = (b, a % b);
            }
            a
        };
        for i in 0..gcd(total, shift) {
            let tmp = *bytes.add(i);
            let mut j = i;
            loop {
                let mut next = j + shift;
                if next >= total {
                    next -= total;
                }
                if next == i {
                    break;
                }
                *bytes.add(j) = *bytes.add(next);
                j = next;
            }
            *bytes.add(j) = tmp;
        }
    }
}

// max_element / min_element
extrema!(__std_max_element_1i, i8, true);
extrema!(__std_max_element_1u, u8, true);
extrema!(__std_max_element_2i, i16, true);
extrema!(__std_max_element_2u, u16, true);
extrema!(__std_max_element_4i, i32, true);
extrema!(__std_max_element_4u, u32, true);
extrema!(__std_max_element_8i, i64, true);
extrema!(__std_max_element_8u, u64, true);
extrema!(__std_max_element_f_, f32, true);
extrema!(__std_max_element_d_, f64, true);

extrema!(__std_min_element_1i, i8, false);
extrema!(__std_min_element_1u, u8, false);
extrema!(__std_min_element_2i, i16, false);
extrema!(__std_min_element_2u, u16, false);
extrema!(__std_min_element_4i, i32, false);
extrema!(__std_min_element_4u, u32, false);
extrema!(__std_min_element_8i, i64, false);
extrema!(__std_min_element_8u, u64, false);
extrema!(__std_min_element_f_, f32, false);
extrema!(__std_min_element_d_, f64, false);

// minmax_element
minmax_extrema!(__std_minmax_element_1i, i8);
minmax_extrema!(__std_minmax_element_1u, u8);
minmax_extrema!(__std_minmax_element_2i, i16);
minmax_extrema!(__std_minmax_element_2u, u16);
minmax_extrema!(__std_minmax_element_4i, i32);
minmax_extrema!(__std_minmax_element_4u, u32);
minmax_extrema!(__std_minmax_element_8i, i64);
minmax_extrema!(__std_minmax_element_8u, u64);
minmax_extrema!(__std_minmax_element_f_, f32);
minmax_extrema!(__std_minmax_element_d_, f64);

// unique
unique!(__std_unique_1, u8);
unique!(__std_unique_2, u16);
unique!(__std_unique_4, u32);
unique!(__std_unique_8, u64);

// search — `_1` and `_2` already exist in MSVC 14.44's msvcprt.lib, so only
// the missing widths are defined here to avoid duplicate symbols.
search!(__std_search_4, u32);
search!(__std_search_8, u64);

// is_sorted_until
sorted_until!(__std_is_sorted_until_1i, i8);
sorted_until!(__std_is_sorted_until_1u, u8);
sorted_until!(__std_is_sorted_until_2i, i16);
sorted_until!(__std_is_sorted_until_2u, u16);
sorted_until!(__std_is_sorted_until_4i, i32);
sorted_until!(__std_is_sorted_until_4u, u32);
sorted_until!(__std_is_sorted_until_8i, i64);
sorted_until!(__std_is_sorted_until_8u, u64);
sorted_until!(__std_is_sorted_until_f, f32);
sorted_until!(__std_is_sorted_until_d, f64);

// string::find_first_not_of
find_first_not_of!(__std_find_first_not_of_trivial_pos_1, u8);
find_first_not_of!(__std_find_first_not_of_trivial_pos_2, u16);
find_first_not_of!(__std_find_first_not_of_trivial_pos_4, u32);
find_first_not_of!(__std_find_first_not_of_trivial_pos_8, u64);

// replace_copy
replace_copy!(__std_replace_copy_1, u8);
replace_copy!(__std_replace_copy_2, u16);
replace_copy!(__std_replace_copy_4, u32);
replace_copy!(__std_replace_copy_8, u64);
