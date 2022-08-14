use std::{collections::HashMap, sync::Mutex};

static MEM: Mutex<Option<HashMap<usize, Vec<u8>>>> = Mutex::new(None);

#[no_mangle]
pub extern "C" fn alloc(len: usize) -> *mut u8 {
    let mut v = Vec::with_capacity(len);
    unsafe {
        v.set_len(len);
    }
    let p = v.as_mut_ptr();
    let k = p as usize;
    let mut m = MEM.lock().unwrap();
    if m.is_none() {
        *m = Some(HashMap::new());
    }
    if let Some(h) = &mut *m {
        h.insert(k, v);
    }
    p
}

#[no_mangle]
pub extern "C" fn free(ptr: *const u8) {
    let k = ptr as usize;
    let mut m = MEM.lock().unwrap();
    if let Some(h) = &mut *m {
        h.remove(&k);
    }
}

pub fn memlen(ptr: *const u8) -> usize {
    let k = ptr as usize;
    let m = MEM.lock().unwrap();
    if let Some(h) = &*m {
        if let Some(v) = h.get(&k) {
            return v.len();
        }
    }
    panic!()
}
