use std::ffi::{CString, c_char, c_int, c_void};
use std::os::unix::ffi::OsStringExt;
use std::path::Path;
use std::sync::Mutex;

use rayon::prelude::*;

#[link(name = "hunspell-1.7")]
unsafe extern "C" {
    fn Hunspell_create(affpath: *const c_char, dpath: *const c_char) -> *mut c_void;
    fn Hunspell_destroy(handle: *mut c_void);
    fn Hunspell_spell(handle: *mut c_void, word: *const c_char) -> c_int;
}

static LIFECYCLE: Mutex<()> = Mutex::new(());

struct Hunspell(*mut c_void);

unsafe impl Send for Hunspell {}

impl Hunspell {
    fn new(dict: &Path) -> Self {
        let path =
            |ext| CString::new(dict.with_extension(ext).into_os_string().into_vec()).unwrap();
        let (aff, dic) = (path("aff"), path("dic"));
        let _guard = LIFECYCLE.lock().unwrap();
        let handle = unsafe { Hunspell_create(aff.as_ptr(), dic.as_ptr()) };
        assert!(!handle.is_null(), "Hunspell_create a échoué");
        Self(handle)
    }

    fn spell(&self, word: &str) -> bool {
        let word = CString::new(word).unwrap();
        unsafe { Hunspell_spell(self.0, word.as_ptr()) != 0 }
    }
}

impl Drop for Hunspell {
    fn drop(&mut self) {
        let _guard = LIFECYCLE.lock().unwrap();
        unsafe { Hunspell_destroy(self.0) }
    }
}

pub fn check_all(dict: &Path, words: &[&str]) -> Vec<bool> {
    let pool = Mutex::new(Vec::new());
    words
        .par_chunks(2048)
        .flat_map_iter(|chunk| {
            let popped = pool.lock().unwrap().pop();
            let h = popped.unwrap_or_else(|| Hunspell::new(dict));
            let ok: Vec<bool> = chunk.iter().map(|w| h.spell(w)).collect();
            pool.lock().unwrap().push(h);
            ok
        })
        .collect()
}
