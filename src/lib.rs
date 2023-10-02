#[global_allocator]
static ALLOC: wee_alloc::WeeAlloc = wee_alloc::WeeAlloc::INIT;

use protobuf::Message;
include!(concat!(env!("OUT_DIR"), "/protos/mod.rs"));
use abi::{Request, Response};

const MAX_ERR_MSG_LEN: usize = 1020;
static mut ERROR_BUFFER: [u8; MAX_ERR_MSG_LEN + 4] = [0; MAX_ERR_MSG_LEN + 4];

// take ownership of a dangling shared memory pointer
fn grab_pointer(ptr: *const u8) -> Box<[u8]> {
    unsafe {
        let size = (*(ptr as *const u32)).to_be() as usize;
        Box::from_raw(std::slice::from_raw_parts_mut(ptr as *mut u8, size + 4)) // +4 for the size prefix
    }
}

#[no_mangle]
pub extern "C" fn __alloc(size: usize) -> *const u8 {
    let buffer = vec![0u8; size].into_boxed_slice();
    Box::into_raw(buffer) as *const u8
}

#[no_mangle]
pub extern "C" fn __dealloc(ptr: *const u8) {
    let _ = grab_pointer(ptr);
}

/// decode a message from a dangling shared memory pointer and free the pointed memory
fn ptr_into_message<M: Message>(ptr: *const u8) -> M {
    M::parse_from_bytes(&grab_pointer(ptr)[4..]).unwrap()
}

/// allocate, encode a message and detach the pointer
fn msg_to_ptr<M: Message>(msg: &M) -> *const u8 {
    let size = msg.compute_size() as u32;
    let mut buf = Vec::with_capacity(size as usize + 4);
    buf.extend_from_slice(&size.to_be_bytes()); // u32 big endian length prefix
    msg.write_to_vec(&mut buf).unwrap();
    Box::into_raw(buf.into_boxed_slice()) as *const u8
}

// The function we'll call from the guest to the host
extern "C" {
    fn host_hello(ptr: *const u8) -> *const u8;
    fn abort(ptr: *const u8);
}

/// Call a host ABI
fn call_abi<M: Message, R: Message, F: Fn(*const u8) -> *const u8>(f: F, request: &M) -> R {
    ptr_into_message(f(msg_to_ptr(request)))
}

fn panic_handler(info: &std::panic::PanicInfo) -> () {
    let msg = info.to_string();
    let msg_bytes = msg.as_bytes();
    let length = msg_bytes.len().min(MAX_ERR_MSG_LEN) as u32; // Ensure we don't exceed buffer size

    unsafe {
        // Point to the start of the ERROR_BUFFER.
        let buffer_start = ERROR_BUFFER.as_mut_ptr();

        // Write the length (as u32, big-endian) to the start of the buffer.
        *(buffer_start as *mut u32) = length.to_be();

        // Use core::ptr::copy to copy the message bytes just after the length.
        core::ptr::copy(msg_bytes.as_ptr(), buffer_start.offset(4), length as usize);

        // Call the abort function.
        abort(buffer_start);

        // This macro will result in the WASM `unreachable` instruction.
        unreachable!();
    }
}

#[no_mangle]
pub extern "C" fn guest_func(ptr: *const u8) -> *const u8 {
    // Decode the request from the host and free it
    let request: Request = ptr_into_message(ptr);

    // Prepare response
    let mut response = Response::new();

    // call the host ABI
    {
        let mut abi_request = Request::new();
        abi_request.message = request.message;
        let abi_response: Response = call_abi(|ptr| unsafe { host_hello(ptr) }, &abi_request);
        response.reply = abi_response.reply;
    }

    // Encode and return the response
    msg_to_ptr(&response)
}

#[no_mangle]
pub extern "C" fn _start() {
    // Set the custom panic handler
    std::panic::set_hook(Box::new(panic_handler));
}
