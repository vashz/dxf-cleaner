//! DXF Cleaner: clean up polluted DXF exports (e.g. from Adobe Illustrator)
//! into continuous lines and true arcs for cutting machines.

pub mod clean;
pub mod dxf_read;
pub mod dxf_write;
pub mod geom;
pub mod spans;

pub use clean::{clean, CleanResult, Options, Shape};
pub use dxf_read::{read_bytes, read_file, Drawing};
pub use dxf_write::{write_dxf, OutputMode};
