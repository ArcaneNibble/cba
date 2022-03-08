class Node:
    def __init__(self, name, inp0, inp1):
        self.name = name
        self.inp0 = inp0
        self.inp1 = inp1
        self.visited = False

    def __repr__(self):
        if self.is_pi():
            return f"Node {self.name}: PI"
        return f"Node {self.name}: {GRAPH[self.inp0].name} & {GRAPH[self.inp1].name}"

    def is_pi(self):
        return self.inp0 is None and self.inp1 is None

GRAPH = [
    Node('a', None, None),
    Node('b', None, None),
    Node('c', None, None),

    Node('d', 0, 1),
    Node('e', 1, 2),

    Node('f', 3, 4),

    Node('x', 0, 5),
]
POs = [6]
# print(GRAPH)

LUTN = 4

def printgraph(graphname, lblfn):
    with open(f'{graphname}.dot', 'w') as f:
        print(f"digraph {graphname} {{", file=f)
        for i in range(len(GRAPH)):
            n = GRAPH[i]
            print(f"{n.name} [label=\"{lblfn(n)}\"];", file=f)
            if n.inp0 is not None:
                print(f"{n.name} -> {GRAPH[n.inp0].name}", file=f)
            if n.inp1 is not None:
                print(f"{n.name} -> {GRAPH[n.inp1].name}", file=f)
        print("}", file=f)

def get_topo_order():
    order = []

    def topo_recurse(ni):
        n = GRAPH[ni]
        if n.visited:
            return

        if n.inp0 is not None:
            topo_recurse(n.inp0)
        if n.inp1 is not None:
            topo_recurse(n.inp1)

        order.append(ni)
        n.visited = True

    for po in POs:
        topo_recurse(po)

    for i in range(len(GRAPH)):
        GRAPH[i].visited = False

    return order

# printgraph('test', lambda x: x.name + "\nhihi")

TOPO_ORDER = get_topo_order()
# print(TOPO_ORDER)

def compute_cuts():
    for ni in TOPO_ORDER:
        n = GRAPH[ni]
        # print(n)

        cuts = [{ni}]

        if not n.is_pi():
            cuts_u = GRAPH[n.inp0].cuts
            cuts_v = GRAPH[n.inp1].cuts
            # print(n, cuts_u, cuts_v)

            for u in cuts_u:
                for v in cuts_v:
                    cuts_merged = u | v
                    if len(cuts_merged) > LUTN:
                        continue
                    cuts.append(cuts_merged)

        # cuts = cuts[::-1]

        i = 0
        # print("~~~~~ all cuts", cuts)
        while i < len(cuts):
            cuti = cuts[i]
            remove = False
            # print(cuti)

            for j in range(len(cuts)):
                if i == j:
                    continue
                cutj = cuts[j]
                # print(cuti, cutj)

                if cutj <= cuti:
                    print(f"! {cuti} is dominated by {cutj}")
                    remove = True

            # print(remove)
            if remove:
                del cuts[i]
            else:
                i += 1

        n.cuts = cuts

def print_cuts(n):
    ret = f"{n.name}\ncuts = ["

    for cut in n.cuts:
        ret += "{"

        for cuti in cut:
            ret += GRAPH[cuti].name + ","

        if ret.endswith(","):
            ret = ret[:-1]
        ret += "},"

    if ret.endswith(","):
        ret = ret[:-1]
    ret += "]"
    return ret

compute_cuts()
printgraph('cuts', print_cuts)

def compute_arrivals():
    for ni in TOPO_ORDER:
        n = GRAPH[ni]

        if n.is_pi():
            n.node_arrival = 0
            assert n.cuts == [{ni}]
            n.cut_arrivals = [9999]
            n.best_cut = None
        else:
            cut_arrivals = [9999]
            print(n.cuts)
            assert n.cuts[0] == {ni}
            assert len(n.cuts) > 1
            # skip trivial cut
            for cut in n.cuts[1:]:
                max_arrival = -1
                for inp_ni in cut:
                    max_arrival = max(max_arrival, GRAPH[inp_ni].node_arrival)
                assert max_arrival >= 0
                cut_arrivals.append(1 + max_arrival)
            n.cut_arrivals = cut_arrivals

            best_cut = None
            node_arrival = 9999

            for i in range(1, len(n.cuts)):
                cut = n.cuts[i]
                cut_arrival = cut_arrivals[i]
                if cut_arrival < node_arrival:
                    node_arrival = cut_arrival
                    best_cut = cut

            assert best_cut is not None
            n.node_arrival = node_arrival
            n.best_cut = set(best_cut)

def print_cuts_arrivals(n):
    ret = print_cuts(n)
    ret += "\ncut arrivals = " + str(n.cut_arrivals)
    ret += "\nnode arrival = " + str(n.node_arrival)
    if n.best_cut is not None:
        ret += "\nbest cut = {"
        for ni in n.best_cut:
            ret += GRAPH[ni].name + ","
        if ret.endswith(","):
            ret = ret[:-1]
        ret += "}"
    return ret

compute_arrivals()
printgraph('cuts_arrivals', print_cuts_arrivals)
