//! Minimal VFS stub — Unix requires a filesystem abstraction.

use crate::spinlock::Spinlock;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum NodeType {
    File,
    Dir,
    CharDevice,
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
        for i in 0..self.count {
            if let Some(n) = self.nodes[i] {
                if n.name == name {
                    return Some(n);
                }
            }
        }
        None
    }

    fn list(&self) {
        for i in 0..self.count {
            if let Some(n) = self.nodes[i] {
                crate::serial::serial_println!("vfs: devfs {}/{} type={:?}", "dev", n.name, n.ty);
            }
        }
    }
}

static DEVFS: Spinlock<DevFs> = Spinlock::new(DevFs::new());

pub fn init() {
    let mut fs = DEVFS.lock();
    fs.add(Node::dev("null", 1, 3));
    fs.add(Node::dev("zero", 1, 5));
    fs.add(Node::dev("tty", 5, 0));
    fs.add(Node::dev("ttyS0", 4, 64));
    drop(fs);
    crate::serial::serial_println!("vfs: devfs initialised");
    DEVFS.lock().list();
    crate::serial::serial_println!("vfs: ramfs placeholder — / mounts as tmpfs (future)");
}

pub fn find_dev(name: &str) -> Option<Node> {
    DEVFS.lock().find(name)
}
