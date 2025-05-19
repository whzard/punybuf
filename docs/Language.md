# The Punybuf Definition language

This DSL is used to create the definitions for types, out of which code is generated for various languages. It's akin to `.proto` files in Protobuf.

## The basics
```pbd
include common
```
The first thing you'd usually want to do is `include common`. The `include` statement does the same thing as its namesake in C, except it also doesn't let you include the same file twice. You may include any pbd file by just putting its path after the include statement: `include ./path/to/file.pbd`. The `common` thing is a bit special in that this file is "baked" right into the punybuf executable. It contains definitions and documentation for all the basic punybuf types.

### Structs
Let's define our first type!
```pbd
User = {
	name: String
	age: UInt
	id: U64
}
```
This is a struct, as defined by the `{}` after the equal sign. It has multiple fields, each of which takes a "child" type. In a binary representation, this struct would look like all its fields put one after the other (yes, the order matters!). Also, some of these fields have a variable size, which can't be known at compile time (`String` and `UInt`).

Let's get our user a few things they can take interest in.
```pbd
User = {
	name: String
	age: UInt
	id: U64
	favorite_things: Array<String>
}
```
The `Array<T>` type is **generic**, meaning it takes another type (`T`) as an argument and uses it somehow. In our case, `Array` serializes to an integer representing its length and then serializes all its items contiguously.

### Enums
Let's let pur user select their mood:
```pbd
UserMood = [
	Neutral, Happy, Sad
]
```
This type is an enum, meaning it supports selecting one of the predefined values. Internally it's represented as a simple 8-bit number. The power of Punybuf's enums comes from their associated values.
```pbd
UserMood = [
	Neutral, Happy, Sad,
	TinkingAbout: String
]
```
If the selected enum variant is `ThinkingAbout`, then a `String` will be serialized right after the enum discriminant (number).

We can integrate our `UserMood` within our `User` like that:
```pbd
User = {
	name: String
	age: UInt
	id: U64
	favorite_things: Array<String>
	current_mood: UserMood
}
```
However, if we're not planning on using `UserMood` anywhere else, we can **inline** that type:
```pbd
User = {
	name: String
	age: UInt
	id: U64
	favorite_things: Array<String>
	current_mood: UserMood [
		Neutral, Happy, Sad,
		TinkingAbout: String
	]
}
```
Inlining allows whoever is reading our file to not have to jump around the file while reading one type. Although the compiler won't let you use your inlined type anywhere else, inlining has no effect on the binary representation, so you can always change you mind later.

Some enums always take values:
```pbd
Cat = { ... }
Entity = [
	User: User, Cat: Cat
]
```
Writing this out like that is very awkward, and that's why we have **value-enums**:
```pbd
Entity = (
	User, Cat
)
```
Value-enums allow you to skip writing out the name of each variant and are also just syntactic sugar that, when desugared, becomes the equivalent to the code we've written above.

### Flag fields
Some fields can be represented as booleans:
```pbd
User = {
	name: String
	age: UInt
	id: U64
	favorite_things: Array<String>
	current_mood: UserMood

	is_friend: Boolean
	likes_cats: Boolean
}
```
The boolean type looks like this: `Boolean = [True, False]` and, if you remember, is represented with an 8-bit integer, like all enums are. This kind of means we're wasting a lot of space storing 2 booleans as two bytes! Instead, we may use **flag fields**!
```pbd
User = {
	name: String
	age: UInt
	id: U64
	favorite_things: Array<String>
	current_mood: UserMood

	flags: U8.{
		is_friend?
		likes_cats?
	}
}
```
When compiled to our favorite programming language, the fields `is_friend` and `likes_cats` will contain an actual boolean type, however, on the wire, they're represented with just one byte, as defined by `U8`. In place of that `U8`, you may put any other number type, that will define what how many flags you may have.

Flags may also carry optional values, that are serialized after the entire flag field. Some users have a preferred color, while others do not. Let's represent this in our type:
```pbd
User = {
	name: String
	age: UInt
	id: U64
	favorite_things: Array<String>
	current_mood: UserMood

	flags: U8.{
		is_friend?
		likes_cats?
		preferred_color?: Color {
			r: U8  g: U8  b: U8 # yes, neither semilcolons nor line breaks are required
		}
	}
}
```

### Aliases
Sometimes, if we're using a type often (or if we want to give more meaning to a type), we might want to alias a type. Creating an alias is as simple as:
```pbd
MultipleStrings = Array<String>
```
We then may use this alias as we'd normally use a type. Some code generators may automatically resolve it for you, some of them won't. You may add a [`@resolve`](Attributes.md#resolve) attribute to always resolve an alias.

This is how Punybuf types work. You may use this knowledge to serialize and deserialize things for storage or transmission. If, however, you're planning on building some kind of RPC system, you might want to consider commands.

### Commands
Commands are especially nice for bidirectional communication. Punybuf is not opinionated on what method you use to send them and is instead only focused on the data it needs to send.

```pbd
sayHello: { name: String } -> String
```
This command takes a struct-like argument. In practice, you "construct" this command like you would a struct, serialize it and send it. After the `->` we have a **return type**. When the command is done being processed by the remote peer, it send you back a special return frame, containing, in our case, a `String`.

Some commands may fail. Actually, all commands may fail. But some can fail in predictable ways:
```pbd
sayHelloToAnyoneButJoe: { name: String } -> String ![InvalidName]
```
The bit after the `!` is exactly what it looks like, an enum! This enum represents the errors that might You can put associated values and use value-enums and all other fancy things, except that unlike usual enums that begin with `0`, *error-enums* begin with `1`. That's because all commands may fail, and the value `0` is reserved for an unexpected error with a string argument. You can think of all enums as being expanded from `![MyError]` to `![UnexpectedError: String, MyError]`.

Commands may also take no argument, or a single type:
```pbd
getMe: () -> User
getUserByID: U64 -> User ![InvalidID]
```
That's it. Over time though, your application will probably need to extend its protocol to support new features. Unless you can guarentee that both ends of a Punybuf RPC channel will stay up-to-date, you might need to support outdated clients. There are two ways to do this.

## Extensions
Exitensions allow adding more fields to already existing structs and adding variants to already existing enums.

### Extending structs
By default, all structs are extensible. Internally, that just means they have an extra [`UInt`](./BinaryFormat.md#uint) at the end, representing the size in bytes of the extensions, so that outdated clients can skip over the unsupported extensions.

The best way of defining extensions is reusing existing [flag fields](#flag-fields). Since flags are just 1s and 0s interpreted in a special way, you can add boolean flags without breaking compatibility.  
If you want to add a flag with an optional value, you can define it using the [`@extension`](Attributes.md#extension) attribute.
```pbd
User = {
	name: String
	age: UInt
	id: U64
	favorite_things: Array<String>
	current_mood: UserMood

	flags: U8.{
		is_friend?
		likes_cats?
		preferred_color?: Color {
			r: U8  g: U8  b: U8
		}
		likes_punybuf? # <-- here
		@extension     # <-- and here
		favorite_pets?: Array<String>
	}
}
```
