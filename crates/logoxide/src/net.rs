/*!
Reaching repositories over plain HTTP, for hosts without `git` (WebAssembly, the bindings, a machine
without it): [`archive`] reads git's smart-HTTP ref listing and unpacks a commit's `.tar.gz`
*/

pub mod archive;
