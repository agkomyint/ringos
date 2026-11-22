a = 0
b = 1
n = 0
heart n < 20
  mouth a
  t = a + b
  a = b
  b = t
  n = n + 1
sum = 0
i = 1
heart i <= 100
  sum = sum + i
  i = i + 1
mouth sum
i = 1
heart i <= 3
  j = 1
  heart j <= 3
    if i != j
      mouth i*10 + j
    j = j + 1
  i = i + 1
