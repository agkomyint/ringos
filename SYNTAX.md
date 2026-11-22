# Ring (for RingOS)
Ring is a tiny language for arcade games. If a feature is not here or in `examples/` and `games/`, it does not exist.

## The 16 KB rule
Every game gets a hard budget, so a whole arcade fits on a tiny machine:
- the compiled game (`.rbc`) is at most **16 KB** (the compiler refuses bigger ones; most games are 0.5-1.5 KB);
- working memory is at most **16 KB**: arrays (8 bytes per `new` cell, **1 byte per `bytes` cell or text**)
  + the value stack (8 bytes per value) + call frames (16 bytes each).
Each live array also costs 48 bytes of bookkeeping and at most 256 arrays may exist at once. Go over and the game stops with a "16 KB memory limit" screen. Use `bytes` for anything that fits in
0-255 (coordinates on a grid, tiles, text): it is 8 times smaller.

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
heart 1 == 1            a game loop that never ends (Esc pauses, hold Esc quits)

for i 0 10              i = 0, 1 ... 9 (the end is re-read every time round)
  mouth i

const SPEED 4           a name for a fixed number (no memory, no slot)
# a comment: from # to the end of the line
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
a = new 10              10 cells, all 0 (working memory is capped at 16 KB in all)
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
| `beep(hz, ms)` | a square-wave tone (HD Audio, else the PC speaker) (`beep(0, 0)` = silence) |
| `save(slot, n)` | keep a number (slot 0-7, per game) across reboots: high scores |
| `load(slot)` | read it back (0 if never saved) |
| `sprite(x, y, "pic", w, scale)` | a picture: one character per pixel, `w` pixels per row, each pixel `scale` x `scale` big. `0-9a-f` = colours 0-15, `.` = see-through. Drawn from the program text, costs no memory |
| `abs(n)`, `min(a, b)`, `max(a, b)` | the usual |
| `overlap(x1,y1,w1,h1, x2,y2,w2,h2)` | 1 if the two rectangles touch, else 0 |
| `tiles(map, map_w, sheet, tile, scroll_x, scroll_y)` | draw a scrolling tile map. `map` is a `bytes` array of tile numbers (`map_w` per row); `sheet` is a `bytes` array holding the tiles one after another, each `tile` x `tile` characters like a sprite |
| `palette(c, r, g, b)` | redefine colour `c` (0-255) with r, g, b each 0-255 (fades, flashes). Reset when the game ends |
| `shake(n)` | jolt the next shown frame by up to `n` pixels (max 16) |

Colours 0-255 (a 256-colour palette, one byte per pixel). 0-15: black, blue, green, cyan, red, magenta, brown, light gray, dark gray, light blue,
light green, light cyan, light red, light magenta, yellow, white. 16-231 are a 6x6x6 colour cube (16 + 36*r + 6*g + b, each 0-5), 232-255 greys.
Keys: arrows move, A = A / Z / Space, B = S / X / Shift, start = Enter, Esc quits to the menu.

## Not in Ring on purpose
Typed input (`ear`, `hear`), files, windows, network, `use`: an arcade game draws, listens to
buttons, and nothing else. That is what keeps games safe to run.
