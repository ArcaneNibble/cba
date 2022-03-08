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

        struct NetAttrs<'a> {
            name: Option<&'a str>,
            driver: Option<&'a Cell>,
        }
        let mut nets: HashMap<usize, NetAttrs> = HashMap::new();

        // read netnames
        for (netname_str, netname_obj) in top_module.netnames.iter() {
            assert_eq!(netname_obj.bits.len(), 1);
            let attr = NetAttrs {
                name: Some(netname_str),
                driver: None,
            };
            let bitidx = match netname_obj.bits[0] {
                BitVal::N(n) => n,
                BitVal::S(_) => panic!("can't be a constant"),
            };
            println!("bit {} is called {}", bitidx, netname_str);
            nets.insert(bitidx, attr);
        }

        let nodes = Vec::new();
        let mut pi = Vec::new();
        let po = Vec::new();

        // read ports
        for (portname, portobj) in top_module.ports.iter() {
            assert_eq!(portobj.bits.len(), 1);
            let bitidx = match portobj.bits[0] {
                BitVal::N(n) => n,
                BitVal::S(_) => panic!("can't be a constant"),
            };

            if let Some(netattr) = nets.get(&bitidx) {
                assert_eq!(netattr.name.unwrap(), portname);
            } else {
                let attr = NetAttrs {
                    name: Some(portname),
                    driver: None,
                };
                nets.insert(bitidx, attr);
            }

            match portobj.direction {
                PortDirection::Input => {
                    let pi_idx = pi.len();
                    println!(
                        "input bit {} is called {} --> pi idx {}",
                        bitidx, portname, pi_idx
                    );
                    pi.push(portname.clone());
                }
                PortDirection::Output => {
                    // purposely don't do anything here yet
                }
                PortDirection::InOut => panic!("can't be inout"),
            }
        }

        Self {
            nodes,
            pi,
            po,

            topo_order: Vec::new(),
        }
    }

    pub fn dump_dot(&self, name: &str) {
        let mut f = File::create(format!("{}.dot", name)).expect("failed to open file");
        f.write(format!("digraph {} {{\n", name).as_bytes())
            .unwrap();

        for pi in &self.pi {
            f.write(format!("{} [label=\"{}\nPI\"];\n", pi, pi).as_bytes())
                .unwrap();
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
    aig.dump_dot("nodes");
}
