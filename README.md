# WynnBuilder
Wynncraft class building calculator & general utility site

![Builder Screenshot](https://user-images.githubusercontent.com/110062564/192047798-d8583fe1-b188-4bc4-85a9-0eecbf10aeef.PNG)

## Features

### WynnBuilder

Takes a build's items as input and returns all relevant information
- Damage numbers
- Spellcosts
- Equip order
- Skillpoints required & left over
- Defensive stats (EHP, EleDefs, etc.)

It also features an ability tree!
![wynnbuilder ability tree](https://user-images.githubusercontent.com/110062564/192048561-2ec91ba7-1793-4d4f-b4d5-6d7c05cfae99.PNG)

Boosts and Powder specials
- Spell boosts, such as Vanish and War Scream
- Powder special buffs
- Damage numbers for specials like Quake and Wind Prison

and more...

### WynnCrafter
![wynncrafter screenshot](https://user-images.githubusercontent.com/110062564/192048366-5112d334-f44b-4853-b337-4184628e505e.PNG)
Crafting recipe calculator


### WynnAtlas
![wynnatlas screenshot](https://user-images.githubusercontent.com/110062564/192048258-23bc0dd7-b417-4c0c-9437-4392315bf85d.PNG)
Fully featured item search!
Use different filters based on:
- Name
- Rarity
- IDs
And more, to find what item suits your build best!

### WynnCustom

Custom item creator


## Why Use It?
- Client sided = no dependence on server requests
- Correct calculations with the correct formulas
- Comprehensive features
- Constantly maintained by class builders

## Documentation

Solver engineering:

- [Exact-search optimization plan](WYNNSOLVER_OPTIMIZATION_PLAN.md)
- [Correctness and benchmark validation](rust/sp_kernel/OPTIMIZATION_VALIDATION.md)
- [Class-build search capacity](rust/sp_kernel/CLASS_BUILD_CAPACITY.md)
- [Rust/WASM benchmark harness](rust/sp_kernel/BENCHMARKING.md)

### Running Locally

The site is a purely client-side static app with **no build step**. However, it uses
`fetch()` to load game data JSON files, so it **cannot** be opened directly from the
filesystem (`file://` URLs will fail with CORS errors). You need a local HTTP server.

```bash
python3 -m http.server 8000
```

Then open: http://localhost:8000
