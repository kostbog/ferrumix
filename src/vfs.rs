//! A minimal virtual filesystem with a `devfs`.
//!
//! Unix needs a namespace for devices before it needs real files, so this is
//! a small table of character devices that `open()` (see [`crate::fd`]) and
//! the shell's `ls` resolve against:
//!
//! ```text
//!   /dev/null    discards writes
//!   /dev/zero    reads as zeroes
//!   /dev/tty     the VGA screen
//!   /dev/ttyS0   the COM1 serial port
//!   /dev/console screen and/or serial, following the console target
//! ```

use crate::spinlock::IntSpinlock;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum NodeType {
    File,
    Dir,
    CharDevice,
}

impl NodeType {
    pub fn as_str(self) -> &'static str {
        match self {
            NodeType::File => "file",
            NodeType::Dir => "dir",
            NodeType::CharDevice => "chardev",
        }
    }
}

#[derive(Clone, Copy)]
pub struct Node {
    pub name: &'static str,
    pub ty: NodeType,
    pub major: u32,
    pub minor: u32,
}

impl Node {
    const fn dev(name: &'static str, major: u32, minor: u32) -> Self {
        Node {
            name,
            ty: NodeType::CharDevice,
            major,
            minor,
        }
    }
}

const DEVFS_MAX: usize = 16;

struct DevFs {
    nodes: [Option<Node>; DEVFS_MAX],
    count: usize,
}

impl DevFs {
    const fn new() -> Self {
        DevFs {
            nodes: [None; DEVFS_MAX],
            count: 0,
        }
    }

    fn add(&mut self, node: Node) {
        if self.count < DEVFS_MAX {
            self.nodes[self.count] = Some(node);
            self.count += 1;
        }
    }

    fn find(&self, name: &str) -> Option<Node> {
        for slot in self.nodes.iter().take(self.count) {
            if let Some(node) = slot {
                if node.name == name {
                    return Some(*node);
                }
            }
        }
        None
    }
}

static DEVFS: IntSpinlock<DevFs> = IntSpinlock::new(DevFs::new());

pub fn init() {
    let mut fs = DEVFS.lock();
    fs.add(Node::dev("null", 1, 3));
    fs.add(Node::dev("zero", 1, 5));
    fs.add(Node::dev("tty", 5, 0));
    fs.add(Node::dev("console", 5, 1));
    fs.add(Node::dev("ttyS0", 4, 64));
    let count = fs.count;
    drop(fs);
    crate::println!("VFS: devfs mounted at /dev, {} devices", count);
}

/// Look up a device by name (without the `/dev/` prefix).
pub fn find_dev(name: &str) -> Option<Node> {
    DEVFS.lock().find(name)
}

/// Print the contents of `/dev` (shell `ls`).
pub fn list() {
    let fs = DEVFS.lock();
    let mut nodes = [None; DEVFS_MAX];
    nodes[..].copy_from_slice(&fs.nodes[..]);
    let count = fs.count;
    drop(fs);
    for slot in nodes.iter().take(count) {
        if let Some(node) = slot {
            crate::println!(
                "/dev/{:<8} {:<8} {}:{}",
                node.name,
                node.ty.as_str(),
                node.major,
                node.minor
            );
        }
    }
}
