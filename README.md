# rustlike (or: Tomb of the Lost Rustaceans)
A simple roguelike game written in Rust with the [tcod](https://github.com/tomassedovic/tcod-rs) library.

This project was originally meant to be part of [RoguelikeDev Does The Complete Roguelike Tutorial 2020](https://www.reddit.com/r/roguelikedev/comments/grccvt/roguelikedev_does_the_complete_roguelike_tutorial/). Will maybe continue messing around with it afterwards.

I'll be using this as an opportunity to learn a bit more about both game design/development and Rust itself, which are two things I've had an interest in for quite a while now, but never got around to experimenting with.

The Rust version of the tutorial used is Tomas Sedovic's, which can be found [here](https://tomassedovic.github.io/roguelike-tutorial/).

# How to play

The objective is simply to see how many floors down into the tomb you can make it before dying.

The game uses a turn-based system; the player moves and then the monsters move. Melee combat is performed by moving into an enemy. Items you find on the ground can be used from your inventory.

## Controls

Movement: Arrow keys (PgUp/PgDn/End/Home for diagonals) *OR* numpad

Wait (skip turn): Numpad 5

Pick up item: G

Inventory (use): I

Inventory (drop): D

Character info: C

Go down stairs: <

Cast targeted spell: Left-click

Main menu: Esc

## Legend

@ = You

o = Orc

T = Troll

% = Dead body

! = Healing potion

\# = Spell scroll

/ = Sword

[ = Shield

< = Staircase (down)
