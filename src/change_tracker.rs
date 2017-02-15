use std::collections::HashMap;
use std::path::{ Path, PathBuf };

use dts_parser::{ BootInfo, Node, Property, Element };

#[derive(Debug)]
pub struct LabelStore<'a> {
    paths: HashMap<PathBuf, Vec<Element<'a>>>,
    labels: HashMap<&'a str, PathBuf>,
}

impl<'a> LabelStore<'a> {
    pub fn new() -> LabelStore<'a> {
        LabelStore { paths: HashMap::new(), labels: HashMap::new() }
    }

    // TODO: somehow keep track of deleted labels so they can be searched for later
    //       while not being used for path lookup during change parsing
    pub fn fill(&mut self, boot_info: &'a BootInfo, ammends: &'a [Node]) {
        self.fill_internal(Path::new("/"), &boot_info.root);
        for node in ammends {
            match *node {
                Node::Existing { ref name, .. } => {
                    if name == "/" {
                        self.fill_internal(Path::new("/"), node);
                    } else if self.labels.contains_key(name.as_str()) {
                        let path = self.labels[name.as_str()].clone();
                        self.fill_internal(&path, node);
                    } else {
                        unimplemented!();
                    }
                }
                Node::Deleted(_) => unreachable!(),
            }
        }
    }

    fn fill_internal(&mut self, path: &Path, node: &'a Node) {
        match *node {
            Node::Deleted(ref name) => {
                let node_path = path.join(name);
                self.delete_labels(&node_path);

                self.paths.entry(path.join(name))
                          .or_insert_with(Vec::new)
                          .push(Element::Node(node));

                let paths: Vec<PathBuf> = self.paths.iter().filter_map(
                    |(key, val)| if key.starts_with(&node_path) {
                        match val.last() {
                            Some(&Element::Node(&Node::Existing { .. })) => {
                                Some(key.to_path_buf())
                            }
                            Some(&Element::Prop(&Property::Existing { .. })) => {
                                Some(key.to_path_buf())
                            }
                            _ => None,
                        }
                    } else {
                        None
                    })
                    .collect();

                for path in &paths {
                    self.delete_labels(path);
                    self.paths.get_mut(path).unwrap().push(Element::Node(node));
                }
            }
            Node::Existing { ref name, ref proplist, ref children, ref labels } => {
                let node_path = path.join(name);
                self.insert_labels(&node_path, labels);

                for prop in proplist {
                    match *prop {
                        Property::Deleted(ref name) => {
                            let label_path = node_path.join(name);
                            self.delete_labels(&label_path);

                            self.paths.entry(label_path)
                                      .or_insert_with(Vec::new)
                                      .push(Element::Prop(prop));
                        },
                        Property::Existing { ref name, ref labels, .. } => {
                            let label_path = node_path.join(name);
                            self.insert_labels(&label_path, labels);

                            self.paths.entry(label_path)
                                      .or_insert_with(Vec::new)
                                      .push(Element::Prop(prop));
                        },
                    }
                }

                for node in children {
                    self.fill_internal(&node_path, node);
                }

                self.paths.entry(node_path)
                          .or_insert_with(Vec::new)
                          .push(Element::Node(node));
            }
        }
    }

    fn delete_labels(&mut self, path: &Path) {
        let mut labels: Vec<&str> = Vec::new();
        for (label, p) in &self.labels {
            if p.starts_with(path) {
                labels.push(label);
            }
        }
        for label in &labels {
            self.labels.remove(label);
        }
    }

    fn insert_labels(&mut self, path: &Path, labels: &'a [String]) {
        for label in labels {
            if !self.labels.contains_key(label.as_str()) {
                self.labels.insert(label, path.to_path_buf());
            } else if self.labels[label.as_str()] != path {
                // TODO: error, duplicate labels
                panic!("Duplicate label \"{}\" at different paths", label);
            }
        }
    }

    pub fn changes_from_path(&self, path: &Path) -> Option<&[Element<'a>]> {
        self.paths.get(path).map(|v| v.as_slice())
    }

    pub fn path_from_label(&self, label: &str) -> Option<&Path> {
        self.labels.get(label).map(|p| p.as_path())
    }
}