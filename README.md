# Bonkers

A simple implementation fo behavior oriented concurrency from
[When Concurrency Matters: Behaviour-Oriented Concurrency](https://doi.org/10.1145/3622852).
The code essentially comes straight from the paper, and is translated fairly literally.

This code is currently work in progress.

## Safety
`bonkers` contains gratuitous quantities of `unsafe` code.  This is partially to aid
in translating the code given in the paper.  However, with the exception of tests involving
the `rayon` threadpool it passes [Miri](https://github.com/rust-lang/miri) without any
undefined behavior over multiple seeds.  However, the current test set is fairly anemic.

