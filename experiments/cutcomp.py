class Node:
    def __init__(self, name, inp0, inp1):
        self.name = name
        self.inp0 = inp0
        self.inp1 = inp1
        self.visited = False

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

printgraph('test', lambda x: x.name + "\nhihi")

topo_order = get_topo_order()
print(topo_order)
