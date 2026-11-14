mod decode;
mod dialogs;
mod handlers;
mod markdown_path;
mod read;
mod requests;
mod save;
mod write_atomic;

pub use decode::{decode_disk, decode_markdown};
pub use dialogs::{DevDialogs, FileDialogs, NativeDialogs, dialogs_from_env};
pub use handlers::{
    FileReadResult, FileSaveResult, open_file, read_file_request, save_file_as_request,
    save_file_request,
};
pub use markdown_path::{is_markdown_path, with_markdown_extension};
pub use read::read_markdown;
pub use requests::{
    MAX_FILE_BYTES, MAX_TEXT_CHARS, SaveAsRequest, SaveRequest, parse_path_request,
    parse_save_as_request, parse_save_request,
};
pub use save::{SaveDecision, decide_save, save_markdown, save_markdown_as};
pub use write_atomic::{AtomicOutcome, write_atomic, write_atomic_unconditionally};
