// Test that all required impls exist.

use std::{
    borrow::Cow,
    path::{Path, PathBuf},
    rc::Rc,
    sync::Arc,
};

use crate::{AbsoluteSystemPath, AbsoluteSystemPathBuf};

macro_rules! all_into {
    ($t:ty, $x:ident) => {
        test_into::<$t, AbsoluteSystemPathBuf>($x.clone());
        test_into::<$t, Box<AbsoluteSystemPath>>($x.clone());
        test_into::<$t, Arc<AbsoluteSystemPath>>($x.clone());
        test_into::<$t, Rc<AbsoluteSystemPath>>($x.clone());
        test_into::<$t, Cow<'_, AbsoluteSystemPath>>($x.clone());
        test_into::<$t, PathBuf>($x.clone());
        test_into::<$t, Box<Path>>($x.clone());
        test_into::<$t, Arc<Path>>($x.clone());
        test_into::<$t, Rc<Path>>($x.clone());
        test_into::<$t, Cow<'_, Path>>($x.clone());
    };
}

#[test]
fn test_borrowed_into() {
    let absolute_system_path = AbsoluteSystemPath::new("/test/path");
    all_into!(&AbsoluteSystemPath, absolute_system_path);
}

#[test]
fn test_owned_into() {
    let absolute_system_path_buf = AbsoluteSystemPathBuf::from("/test/path");
    all_into!(AbsoluteSystemPathBuf, absolute_system_path_buf);
}

fn test_into<T, U>(orig: T)
where
    T: Into<U>,
{
    let _ = orig.into();
}

#[cfg(path_buf_deref_mut)]
#[test]
fn test_deref_mut() {
    // This test is mostly for miri.
    let mut path_buf = AbsoluteSystemPathBuf::from("foobar");
    let _: &mut AbsoluteSystemPath = &mut path_buf;
}

// fn main() {
//     let thing = AbsoluteSystemPath::new("/asdf").join("asdf").join("asdf");
//     let thing2 = AbsoluteSystemPath::new("/asdf").join("asdf").join("asdf");

//     let conversion: &AbsoluteSystemPath =
// Path::new("/asdf").try_into().unwrap();     let iterator:
// Vec<&AbsoluteSystemPath> = thing.ancestors().map(|path| path).collect();

//     println!("{}", thing.display());
//     println!("{}", thing == thing2);
//     println!("{:?}", iterator);
// }