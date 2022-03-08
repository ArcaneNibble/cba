from collections import namedtuple

Node = namedtuple('Node', ['name', 'inp0', 'inp1'])

GRAPH = [
    Node('a', None, None),
    Node('b', None, None),
    Node('c', None, None),

    Node('d', 0, 1),
    Node('e', 1, 2),

    Node('f', 3, 4),

    Node('x', 0, 5),
]
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

printgraph('test', lambda x: x.name + "\nhihi")
