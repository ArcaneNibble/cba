//! Some AIG algorithms

use std::collections::{HashMap, HashSet};
use std::fmt::{self, Display, Formatter};
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
    pub const fn noinv(self) -> Self {
        Self(self.0 & !Self::FLG_INVERT)
    }
    pub const fn inv(self) -> Self {
        Self(self.0 ^ Self::FLG_INVERT)
    }

    pub const fn _any_idx(&self) -> usize {
        (self.0 & Self::MASK_IDX) as usize
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

impl Display for AIGRef {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        if self.is_invert() {
            write!(f, "!")?;
        }

        if self.is_pi() {
            write!(f, "PI")?;
        }

        write!(f, "{}", self._any_idx())
    }
}

const LUT_SZ: usize = 4;

#[derive(Clone, Debug)]
pub struct AIGCut {
    pub refs: HashSet<AIGRef>,
    pub arrival: u32,
    pub area_flow: f32,
}

impl AIGCut {
    fn trivial(nref: AIGRef) -> Self {
        let mut hs = HashSet::new();
        hs.insert(nref);
        Self {
            refs: hs,
            arrival: u32::MAX,
            area_flow: f32::INFINITY,
        }
    }
}

/// AND node in an AIG
#[derive(Clone, Debug)]
pub struct AIGNode {
    pub name: String,
    pub inp0: AIGRef,
    pub inp1: AIGRef,

    /// number of fanouts in the resulting LUT map
    /// will be abused for initial topological order computation as a mark flag
    pub num_fanouts: u32,

    pub cuts: Vec<AIGCut>,
    pub arrival: u32,
    pub area_flow: f32,
}

impl AIGNode {
    pub fn new(inp0: AIGRef, inp1: AIGRef, name: &str) -> Self {
        Self {
            name: name.to_string(),
            inp0,
            inp1,

            num_fanouts: 0,
            cuts: Vec::new(),
            arrival: u32::MAX,
            area_flow: f32::INFINITY,
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
                "$_BUF_" => process_input(nodes, nets, pimap, get_cell_conn(cell, "A")),
                "$_NOT_" => process_input(nodes, nets, pimap, get_cell_conn(cell, "A")).inv(),
                "$_AND_" => {
                    let a = process_input(nodes, nets, pimap, get_cell_conn(cell, "A"));
                    let b = process_input(nodes, nets, pimap, get_cell_conn(cell, "B"));

                    add_cell(nodes, AIGNode::new(a, b, cellname))
                }
                "$_OR_" => {
                    let a = process_input(nodes, nets, pimap, get_cell_conn(cell, "A"));
                    let b = process_input(nodes, nets, pimap, get_cell_conn(cell, "B"));

                    add_cell(nodes, AIGNode::new(a.inv(), b.inv(), cellname)).inv()
                }
                "$_NAND_" => {
                    let a = process_input(nodes, nets, pimap, get_cell_conn(cell, "A"));
                    let b = process_input(nodes, nets, pimap, get_cell_conn(cell, "B"));

                    add_cell(nodes, AIGNode::new(a, b, cellname)).inv()
                }
                "$_NOR_" => {
                    let a = process_input(nodes, nets, pimap, get_cell_conn(cell, "A"));
                    let b = process_input(nodes, nets, pimap, get_cell_conn(cell, "B"));

                    add_cell(nodes, AIGNode::new(a.inv(), b.inv(), cellname))
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
                "$_XNOR_" => {
                    let a = process_input(nodes, nets, pimap, get_cell_conn(cell, "A"));
                    let b = process_input(nodes, nets, pimap, get_cell_conn(cell, "B"));

                    let n1 = add_cell(nodes, AIGNode::new(a, b.inv(), &format!("{}_n1", cellname)));
                    let n2 = add_cell(nodes, AIGNode::new(a.inv(), b, &format!("{}_n2", cellname)));
                    let n3 = add_cell(
                        nodes,
                        AIGNode::new(n1.inv(), n2.inv(), &format!("{}_n3", cellname)),
                    );
                    n3
                }
                "$_ANDNOT_" => {
                    let a = process_input(nodes, nets, pimap, get_cell_conn(cell, "A"));
                    let b = process_input(nodes, nets, pimap, get_cell_conn(cell, "B"));

                    add_cell(nodes, AIGNode::new(a, b.inv(), cellname))
                }
                "$_ORNOT_" => {
                    let a = process_input(nodes, nets, pimap, get_cell_conn(cell, "A"));
                    let b = process_input(nodes, nets, pimap, get_cell_conn(cell, "B"));

                    add_cell(nodes, AIGNode::new(a.inv(), b, cellname)).inv()
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

    pub fn get(&self, nref: AIGRef) -> &AIGNode {
        debug_assert!(!nref.is_pi());
        &self.nodes[nref.idx()]
    }
    pub fn get_mut(&mut self, nref: AIGRef) -> &mut AIGNode {
        debug_assert!(!nref.is_pi());
        &mut self.nodes[nref.idx()]
    }

    fn _topo_visit(&mut self, nref: AIGRef) {
        if nref.is_pi() {
            return;
        }
        if self.get(nref).num_fanouts > 0 {
            return;
        }

        self._topo_visit(self.get(nref).inp0);
        self._topo_visit(self.get(nref).inp1);
        self.topo_order.push(nref.idx());
        self.get_mut(nref).num_fanouts = 1;
    }
    pub fn calc_topo_order(&mut self) {
        self.topo_order.clear();
        self.topo_order.reserve(self.nodes.len());

        for n in &mut self.nodes {
            n.num_fanouts = 0;
        }

        for i in 0..self.po.len() {
            self._topo_visit(self.po[i].1);
        }

        for n in &mut self.nodes {
            n.num_fanouts = 0;
        }
    }

    pub fn dump_dot(&self, name: &str, extra_labels: impl Fn(&Self, &AIGNode) -> String) {
        let mut f = File::create(format!("{}.dot", name)).expect("failed to open file");
        f.write(format!("digraph {} {{\n", name).as_bytes())
            .unwrap();

        for (i, pi) in self.pi.iter().enumerate() {
            f.write(format!("pi{} [shape=triangle label=\"{}\nPI{}\"];\n", i, pi, i).as_bytes())
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
                    "{} [label=\"{}\nidx={}\n{}\"];\n",
                    i,
                    node.name,
                    i,
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

    fn loopdeloop(&mut self) {
        assert!(self.topo_order.len() > 0);

        for &nodei in &self.topo_order {
            let n = &self.nodes[nodei];

            let mut inp0_cuts = if !n.inp0.is_pi() {
                self.get(n.inp0).cuts.clone()
            } else {
                Vec::new()
            };
            inp0_cuts.push(AIGCut::trivial(n.inp0.noinv()));
            let mut inp1_cuts = if !n.inp1.is_pi() {
                self.get(n.inp1).cuts.clone()
            } else {
                Vec::new()
            };
            inp1_cuts.push(AIGCut::trivial(n.inp1.noinv()));

            println!(
                "node {} fanin cuts are {:?} and {:?}",
                n.name, inp0_cuts, inp1_cuts
            );

            let mut new_cuts = Vec::with_capacity(inp0_cuts.len() * inp1_cuts.len());
            for inp0_cut in &inp0_cuts {
                for inp1_cut in &inp1_cuts {
                    let combined_cut: HashSet<AIGRef> =
                        inp0_cut.refs.union(&inp1_cut.refs).copied().collect();

                    if combined_cut.len() > LUT_SZ {
                        continue;
                    }

                    new_cuts.push(AIGCut {
                        refs: combined_cut,
                        arrival: u32::MAX,
                        area_flow: f32::INFINITY,
                    });
                }
            }

            // filter dominated cuts
            let mut i = 0;
            while i < new_cuts.len() {
                let cuti = &new_cuts[i];
                let mut remove = false;

                for j in 0..new_cuts.len() {
                    if i == j {
                        continue;
                    }
                    let cutj = &new_cuts[j];

                    if cutj.refs.is_subset(&cuti.refs) {
                        println!("! {:?} is dominated by {:?}", cuti, cutj);
                        remove = true;
                    }
                }

                if remove {
                    new_cuts.remove(i);
                } else {
                    i += 1;
                }
            }

            // compute cut arrival times
            let mut best_arrival = u32::MAX;
            for cut in &mut new_cuts {
                let mut arrival = 0;
                for cutref in &cut.refs {
                    let this_arrival = if cutref.is_pi() {
                        0
                    } else {
                        self.get(*cutref).arrival
                    };

                    if this_arrival > arrival {
                        arrival = this_arrival
                    }
                }

                cut.arrival = 1 + arrival;
                if cut.arrival < best_arrival {
                    best_arrival = cut.arrival;
                }
            }

            // compute area flow
            // fixme: is this how it's supposed to work?
            let mut best_area_flow = f32::INFINITY;
            for cut in &mut new_cuts {
                let mut af = 0.0;
                for cutref in &cut.refs {
                    let this_af = if cutref.is_pi() {
                        0.0
                    } else {
                        self.get(*cutref).area_flow
                    };

                    af += this_af;
                }

                af += 1.0;
                if n.num_fanouts > 0 {
                    af /= n.num_fanouts as f32;
                }

                cut.area_flow = af;
                if cut.area_flow < best_area_flow {
                    best_area_flow = cut.area_flow;
                }
            }

            println!("--> {:?}", new_cuts);
            self.nodes[nodei].cuts = new_cuts;
            self.nodes[nodei].arrival = best_arrival;
            self.nodes[nodei].area_flow = best_area_flow;
        }

        self.dump_dot("cuts", |_, n| {
            format!(
                "{{{}}}\nbest arrival = {}\nbest area = {}",
                n.cuts
                    .iter()
                    .map(|cut| {
                        format!(
                            "{{{} @ {} AF {}}}",
                            cut.refs
                                .iter()
                                .map(|cutref| { format!("{}", cutref) })
                                .collect::<Vec<String>>()
                                .join(","),
                            cut.arrival,
                            cut.area_flow
                        )
                    })
                    .collect::<Vec<String>>()
                    .join(","),
                n.arrival,
                n.area_flow
            )
        });
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
    let mut aig = AIG::parse_netlist(netlist);
    println!("final AIG is {:#?}", aig);
    aig.dump_dot("nodes", |_, _| "".to_string());

    aig.calc_topo_order();
    println!("topo order is {:?}", aig.topo_order);

    aig.loopdeloop();
}
