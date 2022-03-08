//! Some AIG algorithms

use std::collections::HashMap;
use std::fs::File;
use std::io::Write;
use yosys_netlist_json::*;

/// Reference to a node in an AIG graph. This is just a u64, but it contains
/// extra bits used for things such as invert flags and marking PIs.
#[repr(transparent)]
#[derive(Copy, Clone, PartialEq, Eq, Debug, Hash)]
pub struct AIGRef(u64);

impl AIGRef {
    const FLG_PI: u64 = 1 << 63;
    const FLG_INVERT: u64 = 1 << 62;
    const MASK_IDX: u64 = (1 << 48) - 1;

    pub const fn new(idx: usize, invert: bool, pi: bool) -> Self {
        let mut i = idx as u64;

        if pi {
            i |= Self::FLG_PI
        }
        if invert {
            i |= Self::FLG_INVERT
        }

        Self(i)
    }

    pub const fn is_pi(&self) -> bool {
        self.0 & Self::FLG_PI != 0
    }

    pub const fn is_invert(&self) -> bool {
        self.0 & Self::FLG_INVERT != 0
    }
    pub fn set_invert(&mut self, invert: bool) {
        if invert {
            self.0 |= Self::FLG_INVERT;
        } else {
            self.0 &= !Self::FLG_INVERT;
        }
    }
    pub const fn inv(self) -> Self {
        Self(self.0 ^ Self::FLG_INVERT)
    }

    pub const fn idx(&self) -> usize {
        debug_assert!(!self.is_pi());
        (self.0 & Self::MASK_IDX) as usize
    }

    pub const fn pi_idx(&self) -> usize {
        debug_assert!(self.is_pi());
        (self.0 & Self::MASK_IDX) as usize
    }
}

/// AND node in an AIG
#[derive(Clone, PartialEq, Eq, Debug, Hash)]
pub struct AIGNode {
    pub name: String,
    pub inp0: AIGRef,
    pub inp1: AIGRef,

    /// number of fanouts in the resulting LUT map
    /// will be abused for initial topological order computation as a mark flag
    pub num_fanouts: u32,
}

impl AIGNode {
    pub fn new(inp0: AIGRef, inp1: AIGRef, name: &str) -> Self {
        Self {
            name: name.to_string(),
            inp0,
            inp1,

            num_fanouts: 0,
        }
    }
}

/// Complete AIG, including inputs and outputs
#[derive(Clone, Debug)]
pub struct AIG {
    pub nodes: Vec<AIGNode>,
    pub pi: Vec<String>,
    pub po: Vec<(String, AIGRef)>,

    pub topo_order: Vec<usize>,
}

impl AIG {
    pub fn parse_netlist(netlist: Netlist) -> Self {
        let mut top_module = None;
        for (module_name, module) in netlist.modules.iter() {
            println!("module {}", module_name);
            if let Some(top) = module.attributes.get("top") {
                if let Some(top) = top.to_number() {
                    if top != 0 {
                        println!("this is top");
                        assert!(top_module.is_none());
                        top_module = Some(module);
                    }
                }
            }
        }
        let top_module = top_module.unwrap();

        #[derive(Debug)]
        struct NetAttrs<'a> {
            name: Option<&'a str>,
            driver: Option<&'a Cell>,
            input: bool,
        }
        let mut nets = HashMap::new();

        // read netnames
        for (netname_str, netname_obj) in top_module.netnames.iter() {
            assert_eq!(netname_obj.bits.len(), 1);
            let attr = NetAttrs {
                name: Some(netname_str),
                driver: None,
                input: false,
            };
            let bitidx = match netname_obj.bits[0] {
                BitVal::N(n) => n,
                BitVal::S(_) => panic!("can't be a constant"),
            };
            println!("bit {} is called {}", bitidx, netname_str);
            nets.insert(bitidx, attr);
        }

        // read cells
        fn get_cell_conn(cell: &Cell, conn: &str) -> usize {
            let output = cell
                .connections
                .get(conn)
                .expect(&format!("missing {} connection", conn));
            assert_eq!(output.len(), 1);
            match output[0] {
                BitVal::N(n) => n,
                BitVal::S(_) => panic!("can't be a constant"),
            }
        }

        for (cellname, cellobj) in top_module.cells.iter() {
            println!("cell {} of type {}", cellname, cellobj.cell_type);

            let output = get_cell_conn(cellobj, "Y");
            println!("output wire is {}", output);

            if let Some(attr) = nets.get_mut(&output) {
                assert!(attr.driver.is_none());
                attr.driver = Some(cellobj);
            } else {
                let attr = NetAttrs {
                    name: None,
                    driver: Some(cellobj),
                    input: false,
                };
                nets.insert(output, attr);
            }
        }

        println!("nets {:#?}", nets);

        let mut nodes = Vec::new();
        let mut pi = Vec::new();
        let mut po = Vec::new();

        let mut pimap = HashMap::new();

        // read input ports
        for (portname, portobj) in top_module.ports.iter() {
            assert_eq!(portobj.bits.len(), 1);
            let bitidx = match portobj.bits[0] {
                BitVal::N(n) => n,
                BitVal::S(_) => panic!("can't be a constant"),
            };

            match portobj.direction {
                PortDirection::Input => {
                    if let Some(netattr) = nets.get_mut(&bitidx) {
                        assert_eq!(netattr.name.unwrap(), portname);
                        assert!(netattr.driver.is_none());
                        netattr.input = true;
                    } else {
                        let attr = NetAttrs {
                            name: Some(portname),
                            driver: None,
                            input: true,
                        };
                        nets.insert(bitidx, attr);
                    }

                    let pi_idx = pi.len();
                    println!(
                        "input bit {} is called {} --> pi idx {}",
                        bitidx, portname, pi_idx
                    );
                    pi.push(portname.clone());
                    pimap.insert(bitidx, pi_idx);
                }
                PortDirection::Output => {
                    // purposely don't do anything here yet
                }
                PortDirection::InOut => panic!("can't be inout"),
            }
        }

        fn process_input(
            nodes: &mut Vec<AIGNode>,
            nets: &HashMap<usize, NetAttrs>,
            pimap: &HashMap<usize, usize>,
            yosys_net_idx: usize,
        ) -> AIGRef {
            let net = nets.get(&yosys_net_idx).unwrap();
            println!("processing net {} -> {:?}", yosys_net_idx, net);
            if net.input {
                let pi_idx = pimap.get(&yosys_net_idx).unwrap();
                AIGRef::new(*pi_idx, false, true)
            } else {
                process_cell(
                    nodes,
                    nets,
                    pimap,
                    net.name.unwrap_or(""),
                    net.driver.unwrap(),
                )
            }
        }

        fn process_cell(
            nodes: &mut Vec<AIGNode>,
            nets: &HashMap<usize, NetAttrs>,
            pimap: &HashMap<usize, usize>,
            cellname: &str,
            cell: &Cell,
        ) -> AIGRef {
            fn add_cell(nodes: &mut Vec<AIGNode>, n: AIGNode) -> AIGRef {
                let cell_idx = nodes.len();
                println!("adding AIG cell {:?} @ {}", n, cell_idx);
                nodes.push(n);
                AIGRef::new(cell_idx, false, false)
            }

            match &cell.cell_type as &str {
                "$_AND_" => {
                    let a = process_input(nodes, nets, pimap, get_cell_conn(cell, "A"));
                    let b = process_input(nodes, nets, pimap, get_cell_conn(cell, "B"));

                    add_cell(nodes, AIGNode::new(a, b, cellname))
                }
                "$_XOR_" => {
                    let a = process_input(nodes, nets, pimap, get_cell_conn(cell, "A"));
                    let b = process_input(nodes, nets, pimap, get_cell_conn(cell, "B"));

                    let n1 = add_cell(nodes, AIGNode::new(a, b.inv(), &format!("{}_n1", cellname)));
                    let n2 = add_cell(nodes, AIGNode::new(a.inv(), b, &format!("{}_n2", cellname)));
                    let n3 = add_cell(
                        nodes,
                        AIGNode::new(n1.inv(), n2.inv(), &format!("{}_n3", cellname)),
                    );
                    n3.inv()
                }
                _ => panic!("unknown cell type {}", cell.cell_type),
            }
        }

        // read output ports
        for (portname, portobj) in top_module.ports.iter() {
            assert_eq!(portobj.bits.len(), 1);
            let bitidx = match portobj.bits[0] {
                BitVal::N(n) => n,
                BitVal::S(_) => panic!("can't be a constant"),
            };

            if portobj.direction == PortDirection::Output {
                let net = nets.get(&bitidx).expect("output referenced invalid bit");
                assert!(!net.input);
                assert_eq!(net.name.unwrap(), portname);
                let cell = net.driver.expect("undriven net");

                println!("output {} is from cell {:#?}", portname, cell);
                let po_aigref = process_cell(&mut nodes, &nets, &pimap, portname, cell);
                po.push((portname.clone(), po_aigref));
            }
        }

        Self {
            nodes,
            pi,
            po,

            topo_order: Vec::new(),
        }
    }

    pub fn dump_dot(&self, name: &str, extra_labels: impl Fn(&Self, &AIGNode) -> String) {
        let mut f = File::create(format!("{}.dot", name)).expect("failed to open file");
        f.write(format!("digraph {} {{\n", name).as_bytes())
            .unwrap();

        for (i, pi) in self.pi.iter().enumerate() {
            f.write(format!("pi{} [shape=triangle label=\"{}\nPI\"];\n", i, pi).as_bytes())
                .unwrap();
        }

        for (po_name, po_ref) in &self.po {
            f.write(
                format!(
                    "{} [shape=invtriangle label=\"{}\nPO\"];\n",
                    po_name, po_name
                )
                .as_bytes(),
            )
            .unwrap();

            let inv = if po_ref.is_invert() {
                "[color=blue];"
            } else {
                ""
            };
            f.write(format!("{} -> {} {}\n", po_name, po_ref.idx(), inv).as_bytes())
                .unwrap();
        }

        for (i, node) in self.nodes.iter().enumerate() {
            f.write(
                format!(
                    "{} [label=\"{}\n{}\"];\n",
                    i,
                    node.name,
                    extra_labels(self, node)
                )
                .as_bytes(),
            )
            .unwrap();

            let inv0 = if node.inp0.is_invert() {
                "[color=blue];"
            } else {
                ""
            };
            if node.inp0.is_pi() {
                f.write(format!("{} -> pi{} {}\n", i, node.inp0.pi_idx(), inv0).as_bytes())
                    .unwrap();
            } else {
                f.write(format!("{} -> {} {}\n", i, node.inp0.idx(), inv0).as_bytes())
                    .unwrap();
            }

            let inv1 = if node.inp1.is_invert() {
                "[color=blue];"
            } else {
                ""
            };
            if node.inp1.is_pi() {
                f.write(format!("{} -> pi{} {}\n", i, node.inp1.pi_idx(), inv1).as_bytes())
                    .unwrap();
            } else {
                f.write(format!("{} -> {} {}\n", i, node.inp1.idx(), inv1).as_bytes())
                    .unwrap();
            }
        }

        f.write("}\n".as_bytes()).unwrap();
    }
}

fn main() {
    let args = ::std::env::args().collect::<Vec<_>>();

    if args.len() != 2 {
        println!("Usage: {} input.json", args[0]);
        ::std::process::exit(1);
    }

    // Read the entire file
    let f = File::open(&args[1]).expect("failed to open file");
    let netlist = Netlist::from_reader(f).expect("failed to parse netlist JSON");

    // Make AIG
    let aig = AIG::parse_netlist(netlist);
    println!("final AIG is {:#?}", aig);
    aig.dump_dot("nodes", |_, _| "".to_string());
}
