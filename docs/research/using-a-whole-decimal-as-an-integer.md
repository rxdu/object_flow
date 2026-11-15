# Using a whole decimal as an integer

**The question.** Issue #97, filed 2026-11-14 at 0.25.0, asks for a way to use a decimal that is a whole number where an `int` is needed, and to state that a decimal is whole. A sales line's quantity counts whole units of a product and may be fractional for time sold. As a `decimal`, it cannot drive the `range` that opens a placeholder per unit, and no expression can say it has no fraction. The issue proposes `whole(<decimal>)`, yielding the `int` the decimal equals and no value for one with a fraction. How established platforms convert a decimal to an integer, test that it is whole, and hold a quantity's precision per product: retrieved 2026-11-15, each quotation checked against the page's raw text.

## Findings per source

### Java: an exact conversion that throws on a fraction

`BigDecimal.intValueExact()` "Converts this BigDecimal to an int, checking for lost information. If this BigDecimal has a nonzero fractional part or is out of the possible range for an int result then an ArithmeticException is thrown." `longValueExact()` is the same for a `long`: "ArithmeticException - if this has a nonzero fractional part, or will not fit in a long." The plain `intValue()` drops the fraction silently: "any fractional part of this BigDecimal will be discarded, and if the resulting "BigInteger" is too big to fit in an int, only the low-order 32 bits are returned." The page searched has no `isInteger` method.

### Python: rounding that signals, and `int` that truncates

`decimal`'s `to_integral_exact` is to "Round to the nearest integer, signaling Inexact or Rounded as appropriate if rounding occurs", and `Inexact` "Signals when non-zero digits were discarded during rounding. The rounded result is returned." A result is a `Decimal`, and the signal raises only where its trap is set. The module's own example truncates with `int`: `>>> round(a, 1) Decimal('1.3') >>> int(a) 1`, `a` being 1.34. Its FAQ tests wholeness by comparison: `return d.quantize(Decimal(1)) if d == d.to_integral() else d.normalize()`. The floats have a test, `float.is_integer`: "Return True if the float instance is finite with integral value, and False otherwise". The `decimal` page has no `is_integer`.

### .NET: three silent conversions and a test

`Decimal.ToInt32` truncates: "The return value is the integral part of the decimal value; fractional digits are truncated." `Convert.ToInt32(Decimal)` rounds: "If value is halfway between two whole numbers, the even number is returned; that is, 4.5 is converted to 4, and 5.5 is converted to 6." A C# cast truncates: "When you convert a decimal value to an integral type, this value is rounded toward zero to the nearest integral value." Since .NET 7, `Decimal.IsInteger` "Determines if a value represents an integral number", "so 2.0 and 3.0 will return true while 2.2 and 3.3 will return false."

### PostgreSQL: a cast that rounds, and a scale to test

The numeric-to-integer cast rounds, which the source states and the manual does not: "Convert numeric to int8, rounding if needed." (`numeric.c`, REL_18_STABLE). It is an assignment cast, `castcontext => 'a'` in `pg_cast.dat`, where "a means implicitly in assignment to a target column, as well as explicitly." Ties go away from zero: "When rounding values, the numeric type rounds ties away from zero". `trunc` "Truncates to integer (towards zero)". No function tests wholeness. `min_scale` gives the "Minimum scale (number of fractional decimal digits) needed to represent the supplied value precisely", `min_scale(8.4100) → 2`, where `scale(8.4100) → 4` counts the trailing zeros.

### SQL Server, XPath and rust_decimal: truncation

SQL Server's conversion table has the row `numeric int Truncate`, and its page warns that "the Database Engine sometimes returns a truncated result value, and at other times it returns a rounded value." XPath 3.1's cast to `xs:integer`: "If ST is xs:decimal, xs:float or xs:double, then TV is SV with the fractional part discarded and the value converted to xs:integer." "Thus, casting 3.1456 returns 3 and -17.89 returns -17." rust_decimal 1.43.0 converts by truncation, "Try to convert a Decimal to i64 by truncating and returning the integer component", its `None` being an overflow, as its source shows: `fn to_i64(&self) -> Option<i64> { let d = self.trunc();`. It tests wholeness with `is_integer`: "Returns true if this Decimal number has zero fractional part (is equal to an integer)".

### JavaScript and DMN

`Number.isInteger` "determines whether the passed value is an integer", which ECMAScript defines as "a finite Number value whose mathematical value is in the set of integers." DMN 1.6's FEEL has numbers alone, and where a parameter needs an integer it says so in prose: "position must be a non-zero integer (0 scale number)". It notes that "many different pairs have the same value. For example, value(1,0) = value(10,1) = value(100,2)", and its `decimal(n, scale)` rounds half to even, `decimal(1.5, 0) = 2`. It has no conversion to an integer and no test of one.

### Dynamics 365: whole quantities per product, refused on the line

A product holds "Decimals Supported: Enter a whole number between 0 and 5. If the product can't be divided into fractional quantities, enter 0." "The precision of the Quantity field in the quote, order, or invoice product record is validated against the value in this field if the product doesn't have an associated price list." Where it has one, the price list item's "Quantity Selling Option: Select whether the product or service can be ordered in whole, partial, or both types of quantities", its choices `1 No Control 2 Whole 3 Whole and Fractional`. A line breaking it is an error: "To be able to enter decimal values in the Quantity field, make sure the Quantity Selling Option field isn't set to Whole." The table reference describes the same column, `QuantityDecimal`, otherwise: "Number of decimal places that can be used in monetary amounts for the product." Supply Chain Management refuses a fraction of an integer unit when posting: "a quantity of 0.5 has been specified for an inventory transaction, but only integer quantities are supported." Its packing slip compares the quantity with itself rounded: "if it finds that the rounded shipping quantity doesn't match the actual shipping quantity, you can't generate the packing slip."

### Odoo and SAP: precision per unit, held unevenly

Odoo 17.0 and 18.0 give each unit a rounding: "The computed quantity will be a multiple of this value." "Use 1.0 for a Unit of Measure that cannot be further split, such as a piece." A conversion rounds up by default, `rounding_method='UP'`, and a stock move line is checked against its unit: "# Check here if `ml.quantity` respects the rounding of `ml.product_uom_id`." Odoo 19.0 dropped the rounding per unit: "All Units of Measure share the same rounding precision defined in 'Product Unit'." SAP's units carry decimal places, which one application refuses a fraction by and another does not. A sales order app answers 0.5 of a piece with "Enter a number without decimals", while "In VA01 (GUI), the same input is accepted without error" (KBA 3631618). Of the configuration, "Setting the decimal places for UoMs in this configuration activity does not restrict the values shown or entered in the sales order" (KBA 3536691).

## What the sources share, and where they part

- **A conversion that drops a fraction silently is the norm.** .NET, SQL Server, XPath, rust_decimal and Python's `int` truncate; PostgreSQL and `Convert.ToInt32` round, PostgreSQL in an assignment too. Java alone has an exact conversion, which throws.
- **A test of wholeness** is a function of its own in .NET, rust_decimal, JavaScript and Python's floats, and a comparison of a value with itself rounded in Python's `decimal` FAQ and Dynamics' packing slip. PostgreSQL's `min_scale(x) = 0` is one, *inferred* from its definition.
- **The business applications hold one decimal quantity, and the precision its product or unit allows**: Dynamics per product or price list item, Odoo 17.0 and 18.0 and SAP per unit. None splits the quantity into an integer and a decimal field. Each refuses a fraction in some paths and rounds or accepts it in others.
- *Inferred*: no source gives one function that both converts exactly and is testable without an exception. Java's throws; .NET's and rust_decimal's tests need a second, truncating conversion beside them.

## ObjectFlow beside the sources

- **The language rounds where unavoidable alone.** Rounding is "permitted exactly where it is unavoidable and refused where it would be silent loss", and an expression calls `length`, `trim`, `date`, `add_months`, `entered_at` and `time_in`, with no rounding and no conversion from a decimal to an `int` (`declaration-syntax.md` §8.3). Comparison needs both sides of one type, and `n * 1.0` widens an `int` to a decimal.
- **Reproduced 2026-11-15 at `b5ecfab` (0.26.0)**, on a flow of the repository's own: a `Line` with a `decimal(9,2)` quantity, opening one `Unit` per unit sold. Both checkers refuse `range: inputs.quantity`, "loops over a range of decimal, and a range is an int", and `whole(inputs.quantity)`, "whole is no function the language has" (check 24), the core's through `/sandbox/load`. Both refuse an `int` attribute compared with the quantity, "it compares an int with a decimal of scale 2", and load it widened, `delivered * 1.0 <= quantity`.
- **A value with no value where a rule reads it already exists**: a division by zero is `unknown` (§8.2). The core yields no value and notes it (`eval.rs`'s `failed_arithmetic`); a guard reading it is unsatisfied, `is null` of it is `true`, and an effect's value so left, a loop's `range` among them, refuses the request as `value_in_range` (`request.rs`'s `effect_value`, ADR-0185). An absent `range` runs no step (`flow-format.md` §4.8, `foreach`, ADR-0146). Observed 2026-11-15 at `b5ecfab` on the core, a division by zero read through a derived attribute reaches an effect as plain absence: a loop over it runs no step, a create's argument is not passed, and a `set` of it clears the stored value. A derivation is evaluated apart and keeps its bare value (`eval.rs:1012`), and the reference runner's code does the same (`flowrun.py:1049`).
- **An arithmetic past its type's bound** has no value and refuses wherever it is evaluated, never `unknown` (§8.2, ADR-0205 decision 2), since an `unknown` invariant holds and admitted the write it should refuse.
- **A rule every writer meets** is an invariant of a type, an observation kind or a value, which may call `length`, `trim` and `add_months` (ADR-0164, ADR-0255, ADR-0276).

## What it suggests

A recommendation for the author, not a decision.

1. **Two functions: `whole(<decimal>)`, yielding an `int`, and `is_whole(<decimal>)`, yielding a `bool`.** `whole` is the issue's name. A name after the type, `int(…)`, would read as the casts above, which truncate or round, and as `date(<timestamp>)`, which drops the time of day. `is_whole` follows .NET's `IsInteger` and rust_decimal's `is_integer`.
2. **`whole` of a decimal with a fraction refuses wherever it is evaluated, as an arithmetic past its bound does** (ADR-0205 decision 2), and as Java's `intValueExact` throws. The test stands apart, as .NET's and rust_decimal's do beside their conversions, so a rule names wholeness, `by_time or is_whole(quantity)`, and a loop over `whole(quantity)` never opens fewer units than were sold. Not the no value of a division by zero, which reaches an effect through a derived attribute as plain absence, as observed above, and which an invariant comparing it holds on.
3. **No rounding**, as §8.3 refuses silent loss, which every truncating and rounding conversion above is.
4. **A decimal of any scale alone**, an absent one giving `unknown`, as `length` does, and a whole decimal past 64 bits refusing as an `int` past them does.
5. **Anywhere an expression computes a value, but an indexed derivation**, so `by_time or is_whole(quantity)` can be an invariant every writer meets, as Dynamics validates a line against its product. The generated column of an indexed derivation would compute it in SQL, where PostgreSQL's cast rounds.

## Sources

Retrieved 2026-11-15.

| Source | URL | Version |
|---|---|---|
| Java, `BigDecimal` | https://docs.oracle.com/en/java/javase/21/docs/api/java.base/java/math/BigDecimal.html | Java SE 21 |
| Python, `decimal` | https://docs.python.org/3/library/decimal.html | 3.15.2 |
| Python, built-in types | https://docs.python.org/3/library/stdtypes.html | 3.15.2 |
| .NET, `Decimal.ToInt32` | https://learn.microsoft.com/en-us/dotnet/api/system.decimal.toint32 | current on 2026-11-15 |
| .NET, `Convert.ToInt32` | https://learn.microsoft.com/en-us/dotnet/api/system.convert.toint32 | current on 2026-11-15 |
| .NET, `Decimal.IsInteger` | https://learn.microsoft.com/en-us/dotnet/api/system.decimal.isinteger | .NET 7 and later |
| C#, numeric conversions | https://learn.microsoft.com/en-us/dotnet/csharp/language-reference/builtin-types/numeric-conversions | current on 2026-11-15 |
| PostgreSQL, numeric types | https://www.postgresql.org/docs/current/datatype-numeric.html | 18 |
| PostgreSQL, mathematical functions | https://www.postgresql.org/docs/current/functions-math.html | 18 |
| PostgreSQL, `pg_cast` | https://www.postgresql.org/docs/current/catalog-pg-cast.html | 18 |
| PostgreSQL source, `numeric.c` | https://raw.githubusercontent.com/postgres/postgres/REL_18_STABLE/src/backend/utils/adt/numeric.c | REL_18_STABLE |
| PostgreSQL source, `pg_cast.dat` | https://raw.githubusercontent.com/postgres/postgres/REL_18_STABLE/src/include/catalog/pg_cast.dat | REL_18_STABLE |
| SQL Server, `CAST` and `CONVERT` | https://learn.microsoft.com/en-us/sql/t-sql/functions/cast-and-convert-transact-sql | current on 2026-11-15 |
| XPath and XQuery Functions and Operators 3.1, §19.1.2.4 | https://www.w3.org/TR/xpath-functions-31/ | W3C Recommendation 21 March 2017 |
| rust_decimal, `Decimal` | https://docs.rs/rust_decimal/latest/rust_decimal/struct.Decimal.html | 1.43.0 |
| rust_decimal source, `decimal.rs` | https://raw.githubusercontent.com/paupino/rust-decimal/1.43.0/src/decimal.rs | 1.43.0 |
| MDN, `Number.isInteger` | https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/Number/isInteger | current on 2026-11-15 |
| ECMAScript, notational conventions | https://tc39.es/ecma262/multipage/notational-conventions.html | ECMAScript 2027 draft |
| DMN 1.6 | https://www.omg.org/spec/DMN/1.6/PDF | formal/25-12-02 |
| Dynamics 365 Sales, product bundles | https://learn.microsoft.com/en-us/dynamics365/sales/create-product-bundles-sell-multiple-items-together | current on 2026-11-15 |
| Dynamics 365 Sales, price lists | https://learn.microsoft.com/en-us/dynamics365/sales/create-price-lists-price-list-items-define-pricing-products | current on 2026-11-15 |
| Dynamics 365 Sales, troubleshooting products | https://learn.microsoft.com/en-us/troubleshoot/dynamics-365/sales/troubleshoot-products-issues | current on 2026-11-15 |
| Dataverse, Product table | https://learn.microsoft.com/en-us/dynamics365/developer/reference/entities/product | current on 2026-11-15 |
| Dataverse, Price List Item table | https://learn.microsoft.com/en-us/dynamics365/developer/reference/entities/productpricelevel | current on 2026-11-15 |
| Dynamics 365 SCM, stock keeping unit decimals | https://learn.microsoft.com/en-us/troubleshoot/dynamics-365/supply-chain/inventory/maximum-number-of-decimals-for-stock-keeping-unit-is-0 | current on 2026-11-15 |
| Dynamics 365 SCM, packing slip rounding | https://learn.microsoft.com/en-us/troubleshoot/dynamics-365/supply-chain/warehousing/decimal-rounding-of-physical-updating-quantity-unit-incorrect-error | current on 2026-11-15 |
| Odoo source, `uom_uom.py` | https://raw.githubusercontent.com/odoo/odoo/17.0/addons/uom/models/uom_uom.py | 17.0 |
| Odoo source, `uom_uom.py` | https://raw.githubusercontent.com/odoo/odoo/18.0/addons/uom/models/uom_uom.py | 18.0 |
| Odoo source, `stock_move_line.py` | https://raw.githubusercontent.com/odoo/odoo/17.0/addons/stock/models/stock_move_line.py | 17.0 |
| Odoo source, `uom_uom.py` | https://raw.githubusercontent.com/odoo/odoo/19.0/addons/uom/models/uom_uom.py | 19.0 |
| SAP KBA 3631618 | https://userapps.support.sap.com/sap/support/knowledge/en/3631618 | current on 2026-11-15 |
| SAP KBA 3536691 | https://userapps.support.sap.com/sap/support/knowledge/en/3536691 | current on 2026-11-15 |
