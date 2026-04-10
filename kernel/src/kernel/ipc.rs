// src/kernel/ipc.rs
//! Message-passing IPC for OxideOS.
//!
//! Implements a fixed-size message queue system for use by the window manager
//! and GUI applications.

const MAX_QUEUES: usize = 16;
pub const MAX_MSG_SIZE: usize = 256;
const MSG_QUEUE_DEPTH: usize = 64; // enough for a full compositor frame

#[derive(Copy, Clone)]
pub struct Message {
    pub type_id: u32,
    pub size: u32,
    pub data: [u8; MAX_MSG_SIZE],
}

impl Message {
    pub const fn empty() -> Self {
        Self { type_id: 0, size: 0, data: [0; MAX_MSG_SIZE] }
    }
}

struct MessageQueue {
    id: u32,
    in_use: bool,
    messages: [Message; MSG_QUEUE_DEPTH],
    head: usize,
    tail: usize,
}

impl MessageQueue {
    const fn new() -> Self {
        Self {
            id: 0,
            in_use: false,
            messages: [Message::empty(); MSG_QUEUE_DEPTH],
            head: 0,
            tail: 0,
        }
    }
    
    fn is_empty(&self) -> bool { self.head == self.tail }
    fn is_full(&self) -> bool { (self.tail + 1) % MSG_QUEUE_DEPTH == self.head }
}

static mut QUEUES: [MessageQueue; MAX_QUEUES] = [const { MessageQueue::new() }; MAX_QUEUES];

/// Creates or opens a message queue with the given ID.
pub unsafe fn msgq_create(id: u32) -> i64 {
    let queues = &raw mut QUEUES;
    // Check if exists
    for i in 0..MAX_QUEUES {
        if (*queues)[i].in_use && (*queues)[i].id == id {
            return id as i64; // Already exists, return id
        }
    }
    // Find free slot
    for i in 0..MAX_QUEUES {
        if !(*queues)[i].in_use {
            (*queues)[i].in_use = true;
            (*queues)[i].id = id;
            (*queues)[i].head = 0;
            (*queues)[i].tail = 0;
            return id as i64;
        }
    }
    -4 // ENOMEM
}

/// Sends a message to the specified queue.
pub unsafe fn msgsnd(id: u32, type_id: u32, data: &[u8]) -> i64 {
    if data.len() > MAX_MSG_SIZE { return -22; } // EINVAL
    let queues = &raw mut QUEUES;
    for i in 0..MAX_QUEUES {
        if (*queues)[i].in_use && (*queues)[i].id == id {
            let q = &raw mut (*queues)[i];
            if (*q).is_full() { return -6; } // EAGAIN
            
            let tail = (*q).tail;
            (*q).messages[tail].type_id = type_id;
            (*q).messages[tail].size = data.len() as u32;
            (&mut (*q).messages[tail].data)[..data.len()].copy_from_slice(data);
            
            (*q).tail = (tail + 1) % MSG_QUEUE_DEPTH;
            return 0; // Success
        }
    }
    -2 // ENOENT
}

/// Receives a message from the specified queue.
pub unsafe fn msgrcv(id: u32, msg_out: &mut Message) -> i64 {
    let queues = &raw mut QUEUES;
    for i in 0..MAX_QUEUES {
        if (*queues)[i].in_use && (*queues)[i].id == id {
            let q = &raw mut (*queues)[i];
            if (*q).is_empty() { return -6; } // EAGAIN

            let head = (*q).head;
            *msg_out = (*q).messages[head];

            (*q).head = (head + 1) % MSG_QUEUE_DEPTH;
            return 0; // Success
        }
    }
    -2 // ENOENT
}

/// Destroys a message queue and frees its slot.
pub unsafe fn msgq_destroy(id: u32) -> i64 {
    let queues = &raw mut QUEUES;
    for i in 0..MAX_QUEUES {
        if (*queues)[i].in_use && (*queues)[i].id == id {
            (*queues)[i].in_use = false;
            (*queues)[i].head   = 0;
            (*queues)[i].tail   = 0;
            return 0;
        }
    }
    -2 // ENOENT
}

/// Returns the number of pending messages in the queue (or -2 if not found).
pub unsafe fn msgq_len(id: u32) -> i64 {
    let queues = &raw const QUEUES;
    for i in 0..MAX_QUEUES {
        if (*queues)[i].in_use && (*queues)[i].id == id {
            let q = &(*queues)[i];
            let len = (q.tail + MSG_QUEUE_DEPTH - q.head) % MSG_QUEUE_DEPTH;
            return len as i64;
        }
    }
    -2 // ENOENT
}
