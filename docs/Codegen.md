> Note: "implementation" is a program that takes a Punybuf definition and outputs code to handle it in a real programming language.

`pbd` takes in a `.pbd` file and either generates code directly or outputs the JSON IR with the following schema to stdout.

# JSON Schema
```ts
type Schema = {
	/** Whether this file includes `common` */
	includes_common: boolean
	types: {
		name: string
		layer: number
		is: "struct" | "enum" | "alias"
		generic_args: string[]
		attrs: Attrs
		doc: string
		inline_owner?: String
		/** if this layer of this type is the highest* */
		is_highest_layer: boolean

		/** if alias */
		alias?: Ref

		/** if struct */
		fields?: {
			name: string
			attrs: Attrs
			doc: string
			value: Ref
			flags?: {
				name: string
				attrs: Attrs
				doc: string
				value?: Ref
			}[]
		}[]

		/** if enum */
		variants?: {
			name: string
			discriminant: number
			attrs: Attrs
			doc: string
			value?: Ref
		}[]
	}[]
	commands: {
		name: string
		layer: number
		id: number
		doc: string
		attrs: Attrs
		/** if this layer of this command is the highest* */
		is_highest_layer: boolean

		argument?: {
			is: "ref" | "struct"

			/** if ref */
			ref?: Ref

			/** if struct */
			fields?: {
				name: string
				attrs: Attrs
				doc: string
				value: Ref
				flags?: {
					name: string
					attrs: Attrs
					doc: string
					value?: Ref
				}[]
			}[]
		}
		ret?: Ref
		err: {
			name: string
			/** this begins with a `1`, since the value `0` is reserved for unknown errors */
			discriminant: number
			attrs: Attrs
			doc: string
			value?: Ref
		}[]
	}[]
}

type Ref = [name: string, layer: number | null, generic_args: Ref[], is_highest_layer: boolean]

type Attrs = Record<string, string | null>

// * The `is_highest_layer` property is useful in
//   understanding whether to generate the layer postfix or not.  
```

# Writing a codegen
By default, your codegen should take in the JSON IR with its stdin, to make piping the `pbd` output to it easier: `pbd ./file.pbd | your-codegen --output ./generated.lang`.

Please [review the binary format](BinaryFormat.md) to understand how the format features work and how they should be implemented.