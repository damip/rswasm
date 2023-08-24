#[global_allocator]
static ALLOC: wee_alloc::WeeAlloc = wee_alloc::WeeAlloc::INIT;

use protobuf::Message;
include!(concat!(env!("OUT_DIR"), "/protos/mod.rs"));
use abi::{Request, Response};

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
}

/// Call a host ABI
fn call_abi<M: Message, R: Message, F: Fn(*const u8) -> *const u8>(f: F, request: &M) -> R {
    ptr_into_message(f(msg_to_ptr(request)))
}

pub fn call_host() {
    let mut request = Request::new();
    request.message = "Hello from guest!".to_string();

    let response: Response = call_abi(|ptr| unsafe { host_hello(ptr) }, &request);

    println!("Received from host: {}", response.reply);
}

#[no_mangle]
pub extern "C" fn guest_func(ptr: *const u8) -> *const u8 {
    // Decode the request from the host and free it
    let request: Request = ptr_into_message(ptr);

    // Process the request (for demonstration purposes, just echoing the message back)
    let mut response = Response::new();
    response.reply = format!("Echoing: {}", request.message);

    // Encode and return the response
    msg_to_ptr(&response)
}
