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
    Node('a', None, None),      # 0
    Node('b', None, None),
    Node('c', None, None),
    Node('d', None, None),
    Node('e', None, None),
    Node('f', None, None),

    Node('g', 0, 1),            # 6
    Node('h', 2, 3),
    Node('i', 4, 5),

    Node('j', 6, 2),            # 9
    Node('k', 6, 7),
    Node('l', 7, 8),
    Node('m', 2, 8),

    Node('n', 9, 10),           # 13
    Node('o', 10, 11),
    Node('p', 11, 12),

    Node('q', 13, 14),          # 16
    Node('r', 10, 15),

    Node('s', 16, 17),          # 18
]
POs = [18]
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

printgraph('nodes', lambda x: x.name)

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
            n.af = 0
            n.cut_afs = [9999]
        else:
            cut_arrivals = [9999]
            cut_afs = [9999]
            # print(n.cuts)
            assert n.cuts[0] == {ni}
            assert len(n.cuts) > 1
            # skip trivial cut
            for cut in n.cuts[1:]:
                max_arrival = -1
                af = 0
                for inp_ni in cut:
                    max_arrival = max(max_arrival, GRAPH[inp_ni].node_arrival)
                    af += GRAPH[inp_ni].af
                assert max_arrival >= 0
                cut_arrivals.append(1 + max_arrival)
                cut_afs.append(1 + af)
            n.cut_arrivals = cut_arrivals
            n.cut_afs = cut_afs

            best_cut = None
            best_arrival = 9999
            best_af = 9999

            for i in range(1, len(n.cuts)):
                cut = n.cuts[i]
                cut_arrival = cut_arrivals[i]
                cut_af = cut_afs[i]
                if cut_arrival < best_arrival:
                    best_arrival = cut_arrival
                    best_cut = cut
                    best_af = cut_af
                elif cut_arrival == best_arrival:
                    if cut_af < best_af:
                        best_cut = cut
                        best_af = cut_af

            assert best_cut is not None
            n.node_arrival = best_arrival
            n.best_cut = set(best_cut)
            n.af = best_af

def print_cuts_arrivals(n):
    ret = print_cuts(n)
    ret += "\nbest area = " + str(n.af)
    ret += "\nbest arrival = " + str(n.node_arrival)
    ret += "\nall areas = " + str(n.cut_afs)
    ret += "\nall arrivals = " + str(n.cut_arrivals)
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

def compute_area_flow():
    for ni in TOPO_ORDER:
        n = GRAPH[ni]
        n.num_fanouts = 0


    lutlutlut = []

    def map_lut_at(ni):
        n = GRAPH[ni]
        # print(n.best_cut)
        assert len(n.best_cut) <= LUTN
        inps = []
        for inp in n.best_cut:
            inpn = GRAPH[inp]
            # print(inpn.name)
            inpn.num_fanouts += 1
            if inpn.is_pi():
                inps.append(inp)
            else:
                # print("recurse!")
                lutidx = map_lut_at(inp)
                # print(f"got lut {lutidx}")
                inps.append(-lutidx - 1)
        # print(inps)
        lutidx = len(lutlutlut)
        lutlutlut.append((inps, ni))
        return lutidx

    for po in POs:
        map_lut_at(po)


    for ni in TOPO_ORDER:
        n = GRAPH[ni]

        if n.is_pi():
            n.cut_afs = [9999]
        else:
            cut_afs = [9999]
            assert n.cuts[0] == {ni}
            assert len(n.cuts) > 1
            # skip trivial cut
            for cut in n.cuts[1:]:
                af = 0.0
                for inp in cut:
                    af += GRAPH[inp].af
                af += 1

                if n.num_fanouts != 0:
                    af /= n.num_fanouts

                cut_afs.append(af)
            n.cut_afs = cut_afs


    return lutlutlut

def print_area_flow(n):
    ret = print_cuts(n)
    ret += "\n# fanouts = " + str(n.num_fanouts)
    ret += "\narea flows = " + str(n.cut_afs)
    return ret

lut_mapping = compute_area_flow()
printgraph('cuts_af', print_area_flow)
print(lut_mapping)

def compute_required_times():
    time_max = -1
    for po in POs:
        time_max = max(time_max, GRAPH[po].node_arrival)
    assert time_max >= 0
    print(time_max)

    for ni in TOPO_ORDER:
        GRAPH[ni].required_time = 9999
    for po in POs:
        GRAPH[po].required_time = time_max

    def required_time_at(ni):
        n = GRAPH[ni]
        time_req_new = n.required_time - 1
        if not n.is_pi():
            for inp in n.best_cut:
                inpn = GRAPH[inp]
                time_req_old = inpn.required_time
                inpn.required_time = min(time_req_old, time_req_new)
                required_time_at(inp)

    for po in POs:
        required_time_at(po)

def print_required_times(n):
    ret = n.name
    ret += "\nrequired time = " + str(n.required_time)
    if n.best_cut is not None:
        ret += "\nbest cut = {"
        for ni in n.best_cut:
            ret += GRAPH[ni].name + ","
        if ret.endswith(","):
            ret = ret[:-1]
        ret += "}"
    return ret

compute_required_times()
printgraph('cuts_times', print_required_times)

def recover_area_global():
    ...
