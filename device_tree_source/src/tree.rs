use std::fmt;
use std::collections::HashMap;
use std::collections::hash_map::Entry;

pub trait Labeled {
    fn add_label(&mut self, label: &str);
}

pub trait Offset {
    fn get_offset(&self) -> usize;
}

#[derive(Debug)]
pub struct BootInfo {
    pub reserve_info: Vec<ReserveInfo>,
    pub boot_cpuid: u32,
    pub root: Node,
}

#[derive(Debug)]
pub struct ReserveInfo {
    pub address: u64,
    pub size: u64,
    pub labels: Vec<String>,
}

impl Labeled for ReserveInfo {
    fn add_label(&mut self, label: &str) {
        let label = label.to_owned();
        if !self.labels.contains(&label) {
            self.labels.push(label);
        }
    }
}

#[derive(PartialEq, Eq, Debug)]
pub enum Node {
    Deleted { name: NodeName, offset: usize },
    Existing {
        name: NodeName,

        proplist: HashMap<String, Property>,
        children: HashMap<String, Node>,
        // fullpath: Option<PathBuf>,
        // length to the # part of node_name@#
        // basenamelen: usize,
        //
        // phandle: u32,
        // addr_cells: i32,
        // size_cells: i32,
        labels: Vec<String>,

        offset: usize,
    },
}

impl Node {
    /// Convenience function to get the NodeName no matter what form the
    /// `Node`is in.
    pub fn name(&self) -> &NodeName {
        match *self {
            Node::Deleted { ref name, .. } |
            Node::Existing { ref name, .. } => name,
        }
    }
}

impl Labeled for Node {
    fn add_label(&mut self, label: &str) {
        match *self {
            Node::Deleted { .. } => panic!("Why are you adding a label to a deleted node?!"),
            Node::Existing { ref mut labels, .. } => {
                let label = label.to_owned();
                if labels.contains(&label) {
                    labels.push(label);
                }
            }
        }
    }
}

impl Offset for Node {
    fn get_offset(&self) -> usize {
        match *self {
            Node::Deleted { offset, .. } |
            Node::Existing { offset, .. } => offset,
        }
    }
}

impl fmt::Display for Node {
    // TODO: labels - issue 3
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            Node::Deleted { ref name, .. } => write!(f, "// Node {} deleted", name)?,
            Node::Existing { ref name, ref proplist, ref children, .. } => {
                writeln!(f, "{} {{", name)?;
                for prop in proplist.values() {
                    writeln!(f, "    {}", prop)?;
                }
                for node in children.values() {
                    match *node {
                        Node::Deleted { ref name, .. } => {
                            writeln!(f, "    // Node {} deleted", name)?
                        }
                        Node::Existing { ref name, .. } => writeln!(f, "    {} {{ ... }}", name)?,
                    }
                }
                write!(f, "}}")?;
            }
        }

        Ok(())
    }
}

#[derive(PartialEq, Eq, Debug)]
pub enum NodeName {
    Ref(String),
    Full(String),
}

impl fmt::Display for NodeName {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            NodeName::Ref(ref name) |
            NodeName::Full(ref name) => write!(f, "{}", name),
        }
    }
}

impl NodeName {
    pub fn as_str(&self) -> &str {
        match *self {
            NodeName::Ref(ref name) |
            NodeName::Full(ref name) => name,
        }
    }
}

#[derive(PartialEq, Eq, Debug)]
pub enum Property {
    Deleted { name: String, offset: usize },
    Existing {
        name: String,
        val: Option<Vec<Data>>,
        labels: Vec<String>,
        offset: usize,
    },
}

impl Property {
    /// Convenience function to get the name no matter what form the
    /// `Property`is in.
    pub fn name(&self) -> &str {
        match *self {
           Property::Deleted{ref name, ..} |
           Property::Existing{ref name, ..} => name
        }
    }
}

impl Labeled for Property {
    fn add_label(&mut self, label: &str) {
        match *self {
            Property::Deleted { .. } => {
                panic!("Why are you adding a label to a deleted property?!")
            }
            Property::Existing { ref mut labels, .. } => {
                let label = label.to_owned();
                if labels.contains(&label) {
                    labels.push(label);
                }
            }
        }
    }
}

impl Offset for Property {
    fn get_offset(&self) -> usize {
        match *self {
            Property::Deleted { offset, .. } |
            Property::Existing { offset, .. } => offset,
        }
    }
}

impl fmt::Display for Property {
    // TODO: labels - issue 3
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            Property::Deleted { ref name, .. } => write!(f, "// Property {} deleted", name)?,
            Property::Existing { ref name, ref val, .. } => {
                write!(f, "{}", name)?;
                if let Some(ref data) = *val {
                    if !data.is_empty() {
                        let mut iter = data.iter();
                        write!(f, " = {}", iter.next().unwrap())?;
                        for d in iter {
                            write!(f, ", {}", d)?;
                        }
                    }
                }
                write!(f, ";")?;
            }
        }

        Ok(())
    }
}

#[derive(PartialEq, Eq, Debug)]
pub enum Data {
    Reference(String, Option<u64>),
    String(String),
    Cells(usize, Vec<Cell>),
    ByteArray(Vec<u8>),
}

impl fmt::Display for Data {
    // TODO: labels - issue 3 - issue 6
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            Data::Reference(ref r, _) => write!(f, "&{}", r)?,
            Data::String(ref s) => write!(f, "{}", s)?,
            Data::Cells(bits, ref cells) => {
                if bits != 32 {
                    write!(f, "/bits/ {}", bits)?;
                }
                write!(f, "<")?;
                if !cells.is_empty() {
                    let mut iter = cells.iter();
                    write!(f, "{}", iter.next().unwrap())?;
                    for c in iter {
                        write!(f, "{}", c)?;
                    }
                }
                write!(f, ">")?;
            }
            Data::ByteArray(ref arr) => {
                write!(f, "[ ")?;
                if !arr.is_empty() {
                    let mut iter = arr.iter();
                    write!(f, "{:02X}", iter.next().unwrap())?;
                    for d in iter {
                        write!(f, " {:02X}", d)?;
                    }
                }
                write!(f, " ]")?;
            }
        }

        Ok(())
    }
}

#[derive(PartialEq, Eq, Debug, Clone)]
pub enum Cell {
    Num(u64),
    Ref(String, Option<u64>),
}

impl fmt::Display for Cell {
    // TODO: labels - issue 3 - issue 6
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            Cell::Num(i) => write!(f, "{}", i)?,
            Cell::Ref(ref s, _) => write!(f, "&{}", s)?,
        }

        Ok(())
    }
}
