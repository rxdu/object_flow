#!/usr/bin/env python3
"""The expression language of ObjectFlow flows, parsed into a tree.

The grammar is `docs/design/declaration-syntax.md` §8 and §9, which
`docs/design/flow-format.md` §5 does not change. The tree is shared by the
checks that read what an expression means rather than which names it uses:
the contradiction checker (ADR-0137) and the reference runner of examples
(ADR-0136).

A node is a tuple whose first element names it:
  ("num", value)           an int, or a decimal kept as its text
  ("str", text)            ("bool", value)      ("dur", n, unit)      ("money", ccy, amount)
  ("name", name)           a bare name, resolved by §9.2 where it is used
  ("path", base, member)   base.member
  ("call", fn, args)       fn(args…), fn a name or a path: entered_at(S), this.intervals(m)
  ("agg", kind, distinct, var, collection, where, body)   count(x in c where f: b)
  ("metric", name, binds, over, fresh)                    metric(n, d := e, over last 30 days)
  ("changed", names, event)                               changed_since([a, b], e)
  ("set", items)           {A, B}
  ("not", e)  ("neg", e)   ("isnull", e, negated)   ("in", left, right)
  ("bin", op, left, right) op one of implies or and == != < <= > >= + - * /
  ("if", cond, then, else)

Run as a script, it parses every expression in the design's modules and fails
if any does not parse, which is how the grammar is proven against the record.
"""
import json
import pathlib
import re
import sys

import yaml

ROOT = pathlib.Path(__file__).resolve().parents[1]
FORMAT = ROOT / "docs/design/flow-format"

UNITS = {"s", "min", "h", "day", "days", "week", "weeks"}
AGGREGATES = {"count", "sum", "min", "max", "all", "any", "none", "avg", "median", "percentile"}
KEYWORDS = {"and", "or", "not", "implies", "in", "is", "null", "if", "then", "else", "true", "false"}
COMPARISONS = {"==", "!=", "<", "<=", ">", ">="}
TOKEN = re.compile(r"""
    (?P<ws>\s+)
  | (?P<str>"(?:[^"\\]|\\.)*")
  | (?P<num>\d+(?:\.\d+)?)
  | (?P<name>[A-Za-z_][A-Za-z0-9_]*)
  | (?P<op>==|!=|<=|>=|:=|[<>+\-*/(){}\[\],.:…?])
""", re.X)


class ParseError(ValueError):
    def __init__(self, text, pos, message):
        super().__init__(f"{message}, at {pos + 1} of {text!r}")
        self.pos = pos


def tokenize(text):
    out, pos = [], 0
    while pos < len(text):
        m = TOKEN.match(text, pos)
        if not m:
            raise ParseError(text, pos, f"unexpected character {text[pos]!r}")
        kind = m.lastgroup
        if kind != "ws":
            out.append((kind, m.group(kind), pos))
        pos = m.end()
    out.append(("end", "", len(text)))
    return out


class Parser:
    def __init__(self, text):
        self.text, self.toks, self.i = text, tokenize(text), 0

    def peek(self, k=0):
        return self.toks[min(self.i + k, len(self.toks) - 1)]

    def at(self, value, k=0):
        t = self.peek(k)
        return t[1] == value and t[0] in ("op", "name")

    def take(self, value=None):
        t = self.peek()
        if value is not None and not self.at(value):
            raise ParseError(self.text, t[2], f"expected {value!r}, found {t[1] or 'the end'!r}")
        self.i += 1
        return t

    def fail(self, what):
        t = self.peek()
        raise ParseError(self.text, t[2], f"{what}, found {t[1] or 'the end'!r}")

    # precedence, lowest first: if; implies; or; and; comparison, in, is null; + -; * /; not; paths and calls
    def expr(self):
        if self.at("if"):
            self.take()
            cond = self.expr()
            self.take("then")
            a = self.expr()
            self.take("else")
            return ("if", cond, a, self.expr())
        return self.implies()

    def implies(self):
        left = self.or_()
        if self.at("implies"):
            self.take()
            return ("bin", "implies", left, self.implies())     # right-associative
        return left

    def or_(self):
        left = self.and_()
        while self.at("or"):
            self.take()
            left = ("bin", "or", left, self.and_())
        return left

    def and_(self):
        left = self.comparison()
        while self.at("and"):
            self.take()
            left = ("bin", "and", left, self.comparison())
        return left

    def comparison(self):
        left = self.additive()
        t = self.peek()
        if t[0] == "op" and t[1] in COMPARISONS:
            self.take()
            node = ("bin", t[1], left, self.additive())
        elif self.at("in"):
            self.take()
            node = ("in", left, self.additive())
        elif self.at("is"):
            self.take()
            negated = bool(self.at("not") and self.take())
            self.take("null")
            node = ("isnull", left, negated)
        else:
            return left
        t = self.peek()
        if (t[0] == "op" and t[1] in COMPARISONS) or self.at("in") or self.at("is"):
            self.fail("a comparison does not chain")
        return node

    def additive(self):
        left = self.multiplicative()
        while self.peek()[0] == "op" and self.peek()[1] in ("+", "-"):
            op = self.take()[1]
            left = ("bin", op, left, self.multiplicative())
        return left

    def multiplicative(self):
        left = self.unary()
        while self.peek()[0] == "op" and self.peek()[1] in ("*", "/"):
            op = self.take()[1]
            left = ("bin", op, left, self.unary())
        return left

    def unary(self):
        if self.at("not"):
            self.take()
            return ("not", self.unary())
        if self.at("-"):
            self.take()
            return ("neg", self.unary())
        return self.postfix()

    def postfix(self):
        node = self.primary()
        while True:
            if self.at("."):
                self.take()
                t = self.peek()
                if t[0] != "name":
                    self.fail("a member name follows `.`")
                self.take()
                node = ("path", node, t[1])
            elif self.at("(") and node[0] == "path":
                node = ("call", node, self.arguments())
            else:
                return node

    def arguments(self):
        self.take("(")
        args = []
        while not self.at(")"):
            args.append(self.expr())
            if not self.at(")"):
                self.take(",")
        self.take(")")
        return args

    def primary(self):
        t = self.peek()
        if t[0] == "num":
            self.take()
            nxt = self.peek()
            if nxt[0] == "name" and nxt[1] in UNITS:
                self.take()
                return ("dur", int(t[1]) if "." not in t[1] else t[1], nxt[1])
            return ("num", int(t[1]) if "." not in t[1] else t[1])
        if t[0] == "str":
            self.take()
            return ("str", t[1][1:-1])
        if t[0] == "name":
            if t[1] in ("true", "false"):
                self.take()
                return ("bool", t[1] == "true")
            if re.fullmatch(r"[A-Z]{3}", t[1]) and self.peek(1)[0] == "num":
                self.take()
                return ("money", t[1], self.take()[1])
            if t[1] in KEYWORDS:
                self.fail("an operand")
            self.take()
            if self.at("("):
                return self.call(t[1])
            return ("name", t[1])
        if self.at("("):
            self.take()
            node = self.expr()
            self.take(")")
            return node
        if self.at("{"):
            self.take()
            items = []
            while not self.at("}"):
                items.append(self.expr())
                if not self.at("}"):
                    self.take(",")
            self.take("}")
            return ("set", items)
        self.fail("an operand")

    def call(self, fn):
        comprehension = ((self.at("distinct", 1) and self.peek(2)[0] == "name" and self.at("in", 3))
                         or (self.peek(1)[0] == "name" and self.at("in", 2)))
        if fn in AGGREGATES and not comprehension and (self.at("where", 1) or self.at("distinct", 1)):
            # a metric's aggregate over its own rows: count(where f), count(distinct e [where f]) (§6.9)
            self.take("(")
            distinct = bool(self.at("distinct") and self.take())
            body = None if self.at("where") else self.expr()
            where = None
            if self.at("where"):
                self.take()
                where = self.expr()
            self.take(")")
            return ("agg", fn, distinct, None, None, where, body)
        if fn in AGGREGATES and comprehension:
            self.take("(")
            distinct = bool(self.at("distinct") and self.take())
            var = self.take()[1]
            self.take("in")
            collection = self.additive()
            where = body = None
            if self.at("where"):
                self.take()
                where = self.expr()
            if self.at(":"):
                self.take()
                body = self.expr()
            self.take(")")
            return ("agg", fn, distinct, var, collection, where, body)
        if fn == "metric":
            self.take("(")
            name = self.additive()
            binds, over, fresh = [], None, None
            while self.at(","):
                self.take()
                if self.at("over"):
                    self.take()
                    self.take("last")
                    over = self.primary()
                elif self.at("fresh"):
                    self.take()
                    fresh = self.primary()
                else:
                    dim = self.take()[1]
                    self.take(":=")
                    binds.append((dim, self.expr()))
            self.take(")")
            return ("metric", name, binds, over, fresh)
        if fn == "changed_since":
            self.take("(")
            self.take("[")
            names = []
            while not self.at("]"):
                names.append(self.take()[1])
                if not self.at("]"):
                    self.take(",")
            self.take("]")
            self.take(",")
            event = self.expr()
            self.take(")")
            return ("changed", names, event)
        return ("call", ("name", fn), self.arguments())


def parse(text):
    """The tree of an expression; a YAML boolean or number stands for its literal (flow-format.md §5)."""
    if isinstance(text, bool):
        return ("bool", text)
    if isinstance(text, (int, float)):
        return ("num", text if isinstance(text, int) else str(text))
    p = Parser(str(text))
    node = p.expr()
    if p.peek()[0] != "end":
        p.fail("the end of the expression")
    return node


def expressions(doc):
    """Every expression in a description, with its path: the positions flow.schema.json marks as expressions."""
    schema = json.loads((FORMAT / "flow.schema.json").read_text())
    defs, expression = schema["definitions"], "#/definitions/expression"

    def branches(sch):
        if "$ref" in sch:
            return [sch] if sch["$ref"] == expression else branches(defs[sch["$ref"].rsplit("/", 1)[1]])
        out = [sch]
        for k in ("anyOf", "oneOf", "allOf"):
            for b in sch.get(k, []):
                out += branches(b)
        for k in ("then", "else"):
            if k in sch:
                out += branches(sch[k])
        return out

    found = []

    def walk(node, schemas, path):
        if any(s.get("$ref") == expression for s in schemas) and isinstance(node, (str, bool, int, float)):
            found.append((path, node))
            return
        if isinstance(node, dict):
            for k, v in node.items():
                inner = []
                for sch in schemas:
                    if k in (sch.get("properties") or {}):
                        inner += branches(sch["properties"][k])
                    elif isinstance(sch.get("additionalProperties"), dict):
                        inner += branches(sch["additionalProperties"])
                walk(v, inner, path + (k,))
        elif isinstance(node, list):
            inner = [b for sch in schemas if isinstance(sch.get("items"), dict) for b in branches(sch["items"])]
            for i, v in enumerate(node):
                walk(v, inner, path + (i,))
    walk(doc, branches(schema), ())
    return found


def corpus():
    """Every description in the design: the format's examples, and each YAML block of a design document whose
    top-level keys are a description's, which takes in the modules, their continuations and the section excerpts
    written at the top level."""
    root_keys = set(json.loads((FORMAT / "flow.schema.json").read_text())["properties"])
    out = []
    for p in sorted((FORMAT / "examples").glob("*.yaml")):
        if not p.name.endswith(".examples.yaml"):
            out.append((str(p.relative_to(ROOT)), yaml.safe_load(p.read_text())))
    for p in sorted(ROOT.glob("docs/**/*.md")):
        if "adr" in p.parts:
            continue
        for block in re.findall(r"```yaml\n(.*?)```", p.read_text(), re.S):
            try:
                doc = yaml.safe_load(block)
            except yaml.YAMLError:
                continue
            if isinstance(doc, dict) and doc and set(doc) <= root_keys:
                out.append((str(p.relative_to(ROOT)), doc))
    return out


def self_test():
    """The grammar's precedence and associativity, and what it refuses, each shown by one expression."""
    shapes = [
        ("a or b and c", ("bin", "or", ("name", "a"), ("bin", "and", ("name", "b"), ("name", "c")))),
        ("a implies b implies c", ("bin", "implies", ("name", "a"), ("bin", "implies", ("name", "b"), ("name", "c")))),
        ("a - b - c", ("bin", "-", ("bin", "-", ("name", "a"), ("name", "b")), ("name", "c"))),
        ("x + y * 2 > 3", ("bin", ">", ("bin", "+", ("name", "x"), ("bin", "*", ("name", "y"), ("num", 2))), ("num", 3))),
        ("not internal", ("not", ("name", "internal"))),
        ("eta < now + 2 days", ("bin", "<", ("name", "eta"), ("bin", "+", ("name", "now"), ("dur", 2, "days")))),
        ("total >= USD 19.99", ("bin", ">=", ("name", "total"), ("money", "USD", "19.99"))),
        ("state in {A, B}", ("in", ("name", "state"), ("set", [("name", "A"), ("name", "B")]))),
        ("inputs.engineer.state == User.ACTIVE",
         ("bin", "==", ("path", ("path", ("name", "inputs"), "engineer"), "state"), ("path", ("name", "User"), "ACTIVE"))),
        ("note is not null", ("isnull", ("name", "note"), True)),
        ("none(r in checks where r.failed)", ("agg", "none", False, "r", ("name", "checks"), ("path", ("name", "r"), "failed"), None)),
        ("count(where r.ok)", ("agg", "count", False, None, None, ("path", ("name", "r"), "ok"), None)),
        ("entered_at(MISSING) + 7 days <= now",
         ("bin", "<=", ("bin", "+", ("call", ("name", "entered_at"), [("name", "MISSING")]), ("dur", 7, "days")), ("name", "now"))),
        ("if debit then amount else -amount", ("if", ("name", "debit"), ("name", "amount"), ("neg", ("name", "amount")))),
    ]
    refused = ["a < b < c", "(a and b", "x in [A, B]", "a and", "if a then b", "a == == b", "count(x in )", "a b"]
    ok = True
    for text, want in shapes:
        got = parse(text)
        if got != want:
            ok = False
            print(f"  {text!r} parses as {got}, not {want}")
    for text in refused:
        try:
            parse(text)
            ok = False
            print(f"  {text!r} parses, and the grammar refuses it")
        except ParseError:
            pass
    print(f"grammar: {len(shapes)} shapes as §8.3's precedence gives them, and {len(refused)} malformed expressions refused: "
          + ("yes" if ok else "NO"))
    return ok


def main():
    total, failed = 0, []
    for where, doc in corpus():
        for path, text in expressions(doc):
            total += 1
            try:
                parse(text)
            except ParseError as e:
                failed.append((where, path, e))
    for where, path, e in failed:
        print(f"  {where} {'.'.join(map(str, path))}: {e}")
    print(f"expressions: {total} in the design's descriptions, {total - len(failed)} parsed"
          + ("" if not failed else f", {len(failed)} did not"))
    return 1 if failed or total == 0 or not self_test() else 0


if __name__ == "__main__":
    sys.exit(main())
