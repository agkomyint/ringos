add a b
  back a + b

greet
  mouth "hello from a function"

sumto n
  total = 0
  i = 1
  heart i <= n
    total = total + i
    i = i + 1
  back total

fact n
  if n <= 1
    back 1
  back n * fact(n-1)

greet()
mouth add(2, 3)
mouth sumto(100)
mouth fact(10)
x = 7
mouth add(x, fact(5))
