# Ring (for RingOS)
Ring is a tiny language for arcade games. If a feature is not here or in `examples/` and `games/`, it does not exist.

## Basics
```
mouth 1 + 2 * 3        print a number (or "text"); RingOS has no console, so use it for tests
x = 10                 variables: a name, `=`, a value (whole numbers, 64-bit)
x = x + 1
```
Operators: `+ - * / %`, comparisons `== != < > <= >=` (give 1 or 0), `and` / `or` (short-circuit, loosest).
Unary `-`. Division by zero stops the game with an error screen.

## Control flow (indentation matters, 2 spaces)
```
if x > 5
  mouth "big"
else if x > 2
  mouth "medium"
else
  mouth "small"

heart x < 10            heart = loop while the condition is not 0
  x = x + 1
heart 1 == 1            a game loop that never ends (Esc quits)
```

## Functions
A name, parameter names, an indented block; `back` returns a value; call with `name(a, b)`.
```
add a b
  back a + b
mouth add(2, 3)
```
Functions see only their parameters, not the program's variables. To share game state, pass an array.

## Arrays
```
a = new 10              10 cells, all 0 (up to 1,048,576)
a[0] = 5                index from 0; out of range stops the game with an error
mouth a[0] + size a
b = bytes 4             cells that keep only the low 8 bits: b{0} = 300 stores 44
s = "hello"             text is a bytes array: s{0} is 104
free a                  give an array back (freeing twice is an error)
scope                   arrays made inside the indented block are freed when it ends
  tmp = new 100
  out = keep res        `keep` hands an array out of the scope
```

## Arcade built-ins (the screen is 320x200, scaled to fit the monitor)
| call | does |
|---|---|
| `clear(c)` | fill the screen with colour c |
| `rect(x, y, w, h, c)` | filled rectangle |
| `pixel(x, y, c)` | one pixel |
| `text(x, y, "hi", c)` | 8x8 font (40 columns by 25 rows) |
| `num(x, y, n, c)` | a number as text |
| `key(k)` | 1 while held: 0 left, 1 right, 2 up, 3 down, 4 A, 5 B, 6 start |
| `frame()` | show what you drew and wait for the next frame (60 per second) |
| `time()` | milliseconds since the game started |
| `rnd(n)` | random number from 0 to n-1 |
| `width()`, `height()` | 320, 200 |

Colours 0-15: black, blue, green, cyan, red, magenta, brown, light gray, dark gray, light blue,
light green, light cyan, light red, light magenta, yellow, white. Bigger numbers are raw 0xRRGGBB.
Keys: arrows move, A = A / Z / Space, B = S / X / Shift, start = Enter, Esc quits to the menu.

## Not in Ring on purpose
Typed input (`ear`, `hear`), files, windows, network, `use`: an arcade game draws, listens to
buttons, and nothing else. That is what keeps games safe to run.
