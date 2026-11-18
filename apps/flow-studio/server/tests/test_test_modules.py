"""Each test module binds a name once at its top (docs/LESSONS.md, a module
constant appended to a test file can shadow one of the same name).

A flow appended as a constant under a name an older test's flow has rebinds
it at import, so the older test loads the newer flow and fails, while the new
test run alone passes. An import of a dotted module, `import urllib.request`,
binds its package again, and is no rebinding.
"""

import ast
import pathlib

TESTS = pathlib.Path(__file__).parent


def bound(node: ast.stmt) -> list[str]:
    """The names a statement at a module's top binds."""
    if isinstance(node, ast.Import):
        return [a.asname or a.name for a in node.names
                if a.asname or "." not in a.name]
    if isinstance(node, ast.ImportFrom):
        return [a.asname or a.name for a in node.names]
    if isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef, ast.ClassDef)):
        return [node.name]
    if isinstance(node, ast.Assign):
        return [t.id for t in node.targets if isinstance(t, ast.Name)]
    if isinstance(node, ast.AnnAssign) and isinstance(node.target, ast.Name):
        return [node.target.id]
    return []


def rebound(source: str) -> dict[str, list[int]]:
    """Each name a module's top binds more than once, with the lines binding
    it."""
    lines: dict[str, list[int]] = {}
    for node in ast.parse(source).body:
        for name in bound(node):
            lines.setdefault(name, []).append(node.lineno)
    return {n: at for n, at in lines.items() if len(at) > 1}


def test_each_test_module_binds_a_name_once():
    found = {p.name: r for p in sorted(TESTS.glob("*.py"))
             if (r := rebound(p.read_text()))}
    assert not found, found


def test_a_flow_appended_under_an_older_ones_name_is_found():
    """The check finds what it is for: a flow appended under an older
    flow's name, a test defined twice, and an import a definition hides; and
    a dotted module imported twice is none."""
    appended = ('CARDS = """module: cards"""\n\n\ndef test_a():\n    pass\n\n\n'
                'CARDS = """module: tickets"""\n')
    assert rebound(appended) == {"CARDS": [1, 8]}
    assert rebound("def test_a():\n    pass\n\n\ndef test_a():\n    pass\n") \
        == {"test_a": [1, 5]}
    assert rebound("from x import made\n\n\ndef made():\n    pass\n") \
        == {"made": [1, 4]}
    assert rebound("import urllib.error\nimport urllib.request\n") == {}
