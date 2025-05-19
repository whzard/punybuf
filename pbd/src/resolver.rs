// Beware, weary traveller, since what code lies below is dangerous,
// for your mind and your very soul. With every line, every token,
// stronger and stronger the curse grows, confusing thee in the
// great labyrinth of misdirections and algorithms. Henceforth,
// before you is a choice. Walk away and you will never grasp
// the fucked-upness of the code. Spend too long here, however,
// and you will forever be trapped in the Resolver's domain.

use std::{collections::{HashMap, HashSet, VecDeque}, u32, vec};

use crate::flattener::{PBCommandArg, PBCommandDef, PBEnumVariant, PBField, PBTypeDef, PBTypeRef, PunybufDefinition, PB_CRC};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DependantKind {
	Type, Command
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Dependant {
	pub name: String,
	pub layer: u32,
	pub kind: DependantKind
}

pub struct LayerResolver {
	pub dependencies: HashMap<String, HashSet<Dependant>>,
	pub should_resolve_aliases: bool,
}

#[derive(Clone)]
enum TypeOrCmdDef<'a> {
	TypeDef(&'a PBTypeDef),
	CmdDef(&'a PBCommandDef)
}


impl<'a> TypeOrCmdDef<'a> {
	fn get_layer(&self) -> &u32 {
		match self {
			Self::CmdDef(cmd) => &cmd.layer,
			Self::TypeDef(tp) => tp.get_layer()
		}
	}
}

impl LayerResolver {
	pub fn new(should_resolve_aliases: bool) -> Self {
		Self {
			dependencies: HashMap::new(),
			should_resolve_aliases,
		}
	}
	fn new_dependant(&mut self, refr: &PBTypeRef, dependant: Dependant) {
		if !refr.is_global {
			// a type cannot depend on its own generic arguments.
			// instead the type that depends on it also depends on all
			// generic parameters it specifies
			return;
		}
		let clone = dependant.clone();
		self.new_dependant_string(&refr.reference, dependant);
		for generic_ref in &refr.generics {
			self.new_dependant(generic_ref, clone.clone());
		}
	}
	fn new_dependant_string(&mut self, depends_on: &String, dependant: Dependant) {
		if *depends_on == dependant.name {
			// this is normal, as several builtins are defined like "Type = Type"
			return;
		}

		if let Some(deps) = self.dependencies.get_mut(depends_on) {
			deps.insert(dependant.clone());

		} else {
			let mut hs = HashSet::new();
			hs.insert(dependant.clone());
			self.dependencies.insert(depends_on.clone(), hs);
		}
	}
	fn analyze_struct_dependencies(&mut self, fields: &Vec<PBField>, dependant: Dependant) {
		for field in fields {
			let clone = dependant.clone();
			self.new_dependant(&field.value, clone);
			
			let Some(flags) = &field.flags else { continue };
			let clone_again = dependant.clone();

			for flag in flags {
				let Some(refr) = &flag.value else { continue };

				self.new_dependant(&refr, clone_again.clone());
			}
		}
	}
	fn analyze_enum_dependencies(&mut self, variants: &Vec<PBEnumVariant>, dependant: Dependant) {
		for variant in variants {
			let Some(refr) = &variant.value else { continue };

			self.new_dependant(&refr, dependant.clone());
		}
	}
	fn analyze_type_dependencies(&mut self, tp: &PBTypeDef) {
		let dep = Dependant {
			name: tp.get_name().0.clone(),
			layer: *tp.get_layer(),
			kind: DependantKind::Type
		};
		match tp {
			PBTypeDef::Struct { fields, .. } => {
				self.analyze_struct_dependencies(fields, dep);
			}
			PBTypeDef::Enum { variants, .. } => {
				self.analyze_enum_dependencies(variants, dep);
			}
			PBTypeDef::Alias { alias, .. } => {
				self.new_dependant(&alias, dep);
			}
		}
	}
	fn analyze_command_dependencies(&mut self, cmd: &PBCommandDef) {
		let dep = Dependant {
			name: cmd.name.clone(),
			layer: cmd.layer,
			kind: DependantKind::Command
		};
		if cmd.ret.reference != "Void" {
			self.new_dependant(&cmd.ret, dep.clone());
		}
		match &cmd.argument {
			PBCommandArg::Struct { fields } => {
				self.analyze_struct_dependencies(fields, dep.clone());
			}
			PBCommandArg::Ref(rf) => {
				self.new_dependant(&rf, dep.clone());
			}
			PBCommandArg::None => {}
		}

		self.analyze_enum_dependencies(&cmd.err, dep);
	}
	fn get_highest_layer<'def>(definition: &'def PunybufDefinition, name: &String, limit_layer: u32) -> Option<TypeOrCmdDef<'def>> {
		let mut possible_commands = definition.commands.iter()
			.filter(|cmd| cmd.layer <= limit_layer && cmd.name == *name)
			.collect::<Vec<_>>();
		possible_commands.sort_by_key(|cmd| cmd.layer);
		if let Some(last) = possible_commands.last() {
			return Some(TypeOrCmdDef::CmdDef(&last));
		}

		let mut possible_types = definition.types.iter()
			.filter(|tp| tp.get_layer() <= &limit_layer && tp.get_name().0 == name)
			.collect::<Vec<_>>();
		possible_types.sort_by_key(|tp| tp.get_layer());
		return possible_types.last().map(|v| TypeOrCmdDef::TypeDef(&v));
	}
	fn is_highest_layer(definition: &PunybufDefinition, dependant: &Dependant, limit_layer: u32) -> bool {
		let Some(highest_layer) = Self::get_highest_layer(definition, &dependant.name, limit_layer) else {return true};

		if highest_layer.get_layer() < &dependant.layer {
			panic!("bad state: highest layer of thing {:?} is less than dependant {:?}", highest_layer.get_layer(), dependant)
		}
		return highest_layer.get_layer() == &dependant.layer;
	}
	fn get_type_from_dependant<'def>(definition: &'def PunybufDefinition, dependant: &Dependant) -> Option<&'def PBTypeDef> {
		definition.types.iter().find(|tp| tp.get_name().0 == &dependant.name && tp.get_layer() == &dependant.layer)
	}
	fn get_command_from_dependant<'def>(definition: &'def PunybufDefinition, dependant: &Dependant) -> Option<&'def PBCommandDef> {
		definition.commands.iter().find(|cmd| cmd.name == dependant.name && cmd.layer == dependant.layer)
	}
	fn track_changes(&mut self, definition: &mut PunybufDefinition, index: usize) -> () {
		let changed_type = &definition.types[index];

		let mut new_types = vec![];
		let mut new_commands = vec![];
		let Some(dependants) = self.dependencies.get(changed_type.get_name().0) else { return };
		for dependant in dependants {
			if &dependant.layer >= changed_type.get_layer() {
				continue;
			}
			if !Self::is_highest_layer(definition, dependant, *changed_type.get_layer()) {
				// Eliminate unnecessary generations:
				// B#0 depends on A. A got changed in A#2, but the latest B is B#1 that doesn't depend on A.
				//  => no need to generate a new B.
				continue;
			}

			match dependant.kind {
				DependantKind::Type => {
					let mut new_type = Self::get_type_from_dependant(definition, dependant)
						.expect("dependant doesn't exist?") // trust the verifier 
						.clone();
					match &mut new_type {
						PBTypeDef::Alias { layer, .. } |
						PBTypeDef::Enum { layer, .. } |
						PBTypeDef::Struct { layer, .. } => {
							*layer = *changed_type.get_layer();
						}
					}
					new_types.push(new_type);
				}
				DependantKind::Command => {
					let mut new_cmd = Self::get_command_from_dependant(definition, dependant).unwrap().clone(); // trust the verifier
					new_cmd.layer = *changed_type.get_layer();
					new_cmd.command_id = PB_CRC.checksum(format!("{}.{}", new_cmd.name, new_cmd.layer).as_bytes());
					new_commands.push(new_cmd);
				}
			}
		}

		for cmd in &new_commands {
			self.analyze_command_dependencies(cmd);
		}
		for tp in &new_types {
			self.analyze_type_dependencies(tp);
		}

		definition.types.append(&mut new_types);
		definition.commands.append(&mut new_commands);
	}
	fn check_if_global_reference(refr: &mut PBTypeRef, generics: &Vec<String>) {
		refr.is_global = !generics.contains(&refr.reference);
		for generic_refr in &mut refr.generics {
			Self::check_if_global_reference(generic_refr, generics);
		}
	}
	/// This function consumes the `LayerResolver` so that it can't be re-used
	/// for other `PunybufDefinition`s, since its `HashMap` may get filled with garbage
	// `LayerResolver` in general has quite a weird singature and so possibly
	// TODO: refactor this so that `PunybufDefinition` is present on the struct itself
	// (lifetimes get messy sometimes)
	pub fn resolve(mut self, definition: &mut PunybufDefinition) {
		for index in 0..definition.types.len() {
			let tp = &mut definition.types[index];
			match tp {
				PBTypeDef::Alias { alias, generic_args, .. } => {
					Self::check_if_global_reference(alias, &generic_args);
				}
				PBTypeDef::Enum { variants, generic_args, .. } => {
					for var in variants {
						if let Some(refr) = &mut var.value {
							Self::check_if_global_reference(refr, &generic_args);
						}
					}
				}
				PBTypeDef::Struct { fields, generic_args, .. } => {
					for fld in fields {
						Self::check_if_global_reference(&mut fld.value, &generic_args);
						let Some(flags) = &mut fld.flags else { continue };
						for flag in flags {
							let Some(val) = &mut flag.value else { continue };
							Self::check_if_global_reference(val, &generic_args);
						}
					}
				}
			}
			// commands can't have generics
		}

		for tp in &definition.types {
			self.analyze_type_dependencies(tp);
		}
		for cmd in &definition.commands {
			self.analyze_command_dependencies(cmd);
		}
		let mut index = 0;
		while index < definition.types.len() {
			self.track_changes(definition, index);
			index += 1;
		}

		self.resolve_references(definition);
	}
	fn resolve_alias(refr: &PBTypeRef, tp: &PBTypeDef) -> PBTypeRef {
		let PBTypeDef::Alias { alias, generic_args, .. } = tp else {
			panic!("bad state: @resolve may only be used on aliases");
		};

		let mut result = alias.clone();

		if !result.is_global {
			// @resolve
			// Opaque<T> = T
			// ...
			// Other = { refr: Opaque<InputRef> }
			let arg_index = generic_args.iter().position(|arg| arg == &result.reference)
				.expect("bad state: can't find a generic argument");
			let input_ref = &refr.generics[arg_index]; // should be 0 to be honest
			return input_ref.clone();
		}

		for output_generic_param in &mut result.generics {
			// @resolve
			// Alias<T, Y> = Output<T, String, Y>
			// ...
			// Other = { refr: Alias<InputRef, InputRef> }
			if output_generic_param.is_global { continue }

			let arg_index = generic_args.iter().position(|arg| arg == &output_generic_param.reference)
				.expect("bad state: can't find a generic argument");
			let input_ref = &refr.generics[arg_index];

			*output_generic_param = input_ref.clone();
		}

		result
	}
	fn resolve_is_highest_layer(&self, definition: &PunybufDefinition, name: &String, parent_layer: u32) -> bool {
		let highest_layer = Self::get_highest_layer(definition, name, u32::MAX)
			.expect("can't find highest layer, reference not resolved"); // trust the validator + resolver
		*highest_layer.get_layer() == parent_layer
	}

	fn resolve_reference(&self, definition: &PunybufDefinition, refr: &PBTypeRef, parent_layer: u32, tries: usize) -> Option<ResolvedReference> {
		if tries > 100 {
			panic!("circular reference")
		}
		if !refr.is_global || refr.reference == "Void" {
			return None;
		}
		let with_correct_layer = Self::get_highest_layer(&*definition, &refr.reference, parent_layer)
			.expect("can't find highest layer, reference not resolved"); // trust the validator + resolver

		if let TypeOrCmdDef::TypeDef(tp) = with_correct_layer {
			if tp.get_attrs().contains_key("@resolve") && self.should_resolve_aliases {
				let mut dealias = Self::resolve_alias(&refr, tp);
				if let Some(resolution) = self.resolve_reference(definition, &dealias, parent_layer, tries + 1) {
					self.apply_resolution_to_reference(&mut dealias, resolution);
				}
				return ResolvedReference::Dealias(dealias).into();
			}
		};

		let highest_layer = Self::get_highest_layer(&*definition, &refr.reference, u32::MAX)
			.expect("can't find highest layer, reference not resolved"); // trust the validator + resolver

		let mut generics = VecDeque::new();

		for generic_refr in &refr.generics {
			generics.push_back(self.resolve_reference(definition, generic_refr, parent_layer, tries + 1));
		}

		ResolvedReference::Resolved {
			resolved_layer: *with_correct_layer.get_layer(),
			is_highest_layer: *highest_layer.get_layer() == *with_correct_layer.get_layer(),
			generics,
		}.into()
	}

	fn apply_resolution_to_reference(&self, refr: &mut PBTypeRef, resolution: ResolvedReference) {
		match resolution {
			ResolvedReference::Dealias(dealias) => {
				*refr = dealias;
			}
			ResolvedReference::Resolved { resolved_layer, is_highest_layer, mut generics } => {
				refr.resolved_layer = Some(resolved_layer);
				refr.is_highest_layer = is_highest_layer;
				for generic in &mut refr.generics {
					let Some(resolved) = generics.pop_front()
						.expect("bad state: resolution's generics are smaller than that of the original reference")
						else { continue };

					self.apply_resolution_to_reference(generic, resolved);
				}
			}
		}
	}

	fn resolve_fields(&self, definition: &PunybufDefinition, fields: &Vec<PBField>, layer: u32) -> VecDeque<ResolvedField> {
		let mut result = VecDeque::new();
		for field in fields {
			let flags = field.flags.as_ref().map(|flags| {
				flags.iter().map(|flag| {
					flag.value.as_ref().and_then(|refr| {
						self.resolve_reference(definition, &refr, layer, 0)
					})
				}).collect()
			});
			result.push_back(ResolvedField {
				refr: self.resolve_reference(definition, &field.value, layer, 0),
				flags
			});
		}
		result
	}

	fn apply_resolution_to_fields(&self, fields: &mut Vec<PBField>, mut res_fields: VecDeque<ResolvedField>) {
		for field in fields {
			let res_field = res_fields.pop_front().unwrap();
			if let Some(res_refr) = res_field.refr {
				self.apply_resolution_to_reference(&mut field.value, res_refr);
			}
			let Some(flags) = &mut field.flags else { continue };
			let mut res_flags = res_field.flags.unwrap();
			for flag in flags {
				let res_flag = res_flags.pop_front().unwrap();
				let Some(flag_ref) = &mut flag.value else { continue };
				let Some(res_flag) = res_flag else { continue };
				self.apply_resolution_to_reference(flag_ref, res_flag);
			}
		}
	}
	fn apply_resolution_to_variants(&self, variants: &mut Vec<PBEnumVariant>, mut res_variants: VecDeque<Option<ResolvedReference>>) {
		for variant in variants {
			let res_variant = res_variants.pop_front().unwrap();
			let Some(refr) = &mut variant.value else { continue };
			let Some(res_refr) = res_variant else { continue };
			self.apply_resolution_to_reference(refr, res_refr);
		}
	}

	fn resolve_references(&self, definition: &mut PunybufDefinition) {
		// This function is quite a big hack. It performs a lot of
		// unnecessary allocation and has to have a whole new type for itself
		// and is generally inefficient (for the sake of *relative* beauty).
		// This is all done to fight the borrow checker, which i am not happy about
		// below all this mess, you will find an unsafe version of this function
		// it's not only unsafe, it's also just plain undefined behavior
		// (it works though!) more details below
		// 
		// pull requests welcome, if you want to eliminate the unnecessary allocations
		// (provided your code won't be uglier than this lol)
		let mut type_resolution = VecDeque::<ResolvedTypeDef>::new();
		for tp in &definition.types {
			let is_highest_layer = self.resolve_is_highest_layer(definition, tp.get_name().0, *tp.get_layer());
			match tp {
				PBTypeDef::Alias { alias, layer, .. } => {
					type_resolution.push_back(ResolvedTypeDef {
						is_highest_layer,
						data: ResolvedTypeDefData::Alias {
							refr: self.resolve_reference(definition, alias, *layer, 0)
						}
					});
				}
				PBTypeDef::Struct { fields, layer, .. } => {
					type_resolution.push_back(ResolvedTypeDef {
						is_highest_layer,
						data: ResolvedTypeDefData::Struct {
							fields: self.resolve_fields(definition, fields, *layer)
						}
					});
				}
				PBTypeDef::Enum { variants, layer, .. } => {
					// this code is simple enough that we don't have a
					// "resolve_variants" function, even though that would be symmetric
					let mut resolved_variants = VecDeque::new();
					for variant in variants {
						resolved_variants.push_back(variant.value.as_ref().and_then(|refr| {
							self.resolve_reference(definition, &refr, *layer, 0)
						}));
					}
					type_resolution.push_back(ResolvedTypeDef {
						is_highest_layer,
						data: ResolvedTypeDefData::Enum {
							variants: resolved_variants
						}
					});
				}
			}
		}

		for tp in &mut definition.types {
			let resolution = type_resolution.pop_front().expect("bad state: type resolution vec is too short");
			match tp {
				PBTypeDef::Alias { is_highest_layer, .. } |
				PBTypeDef::Struct { is_highest_layer, .. } |
				PBTypeDef::Enum { is_highest_layer, .. } => {
					*is_highest_layer = resolution.is_highest_layer;
				}
			}
			match tp {
				PBTypeDef::Alias { alias, .. } => {
					let ResolvedTypeDefData::Alias { refr } = resolution.data else {
						panic!("bad state: resolution's alias != real alias")
					};
					let Some(refr) = refr else { continue };
					self.apply_resolution_to_reference(alias, refr);
				}
				PBTypeDef::Struct { fields, .. } => {
					let ResolvedTypeDefData::Struct { fields: res_fields } = resolution.data else {
						panic!("bad state: resolution's struct != real struct")
					};
					self.apply_resolution_to_fields(fields, res_fields);
				}
				PBTypeDef::Enum { variants, .. } => {
					let ResolvedTypeDefData::Enum { variants: res_variants } = resolution.data else {
						panic!("bad state: resolution's enum != real enum")
					};
					self.apply_resolution_to_variants(variants, res_variants);
				}
			}
		}

		let mut cmd_resolution = VecDeque::<ResolvedCommand>::new();
		for cmd in &definition.commands {
			let is_highest_layer = self.resolve_is_highest_layer(definition, &cmd.name, cmd.layer);
			cmd_resolution.push_back(ResolvedCommand {
				is_highest_layer,
				ret: self.resolve_reference(&definition, &cmd.ret, cmd.layer, 0),
				err: cmd.err.iter().map(|variant| {
					variant.value.as_ref().and_then(|refr| {
						self.resolve_reference(definition, &refr, cmd.layer, 0)
					})
				}).collect(),
				arg: match &cmd.argument {
					PBCommandArg::Ref(refr) => {
						ResolvedCommandArg::Ref(self.resolve_reference(definition, &refr, cmd.layer, 0))
					}
					PBCommandArg::None => {
						ResolvedCommandArg::Ref(None)
					}
					PBCommandArg::Struct { fields } => {
						ResolvedCommandArg::Struct {
							fields: self.resolve_fields(definition, &fields, cmd.layer)
						}
					}
				},
			});
		}

		for cmd in &mut definition.commands {
			let res_cmd = cmd_resolution.pop_front().unwrap();
			cmd.is_highest_layer = res_cmd.is_highest_layer;
			match &mut cmd.argument {
				PBCommandArg::None => {},
				PBCommandArg::Ref(refr) => {
					let ResolvedCommandArg::Ref(resolution) = res_cmd.arg else {
						panic!("bad state: resolved argument != real argument")
					};
					let Some(resolution) = resolution else { continue };
					self.apply_resolution_to_reference(refr, resolution);
				}
				PBCommandArg::Struct { fields } => {
					let ResolvedCommandArg::Struct { fields: res_fields } = res_cmd.arg else {
						panic!("bad state: resolved argument != real argument")
					};
					self.apply_resolution_to_fields(fields, res_fields);
				}
			}
			if let Some(ret) = res_cmd.ret {
				self.apply_resolution_to_reference(&mut cmd.ret, ret);
			}
			self.apply_resolution_to_variants(&mut cmd.err, res_cmd.err);
		}
	}

	// here be dragons:

	/// the `refr` argument is actually being **mutated**, even though it's said to be a `*const _`.
	/// miri says that this is undefined behavior and i believe it, but don't care enough to fight with the
	/// borrow checker instead of writing useful code. for more, see the comment under this function.
	unsafe fn resolve_reference_unsafe(&self, definition: *const PunybufDefinition, refr: *const PBTypeRef, parent_layer: u32, tries: usize) {
		if tries > 100 {
			panic!("circular reference")
		}
		// SAFETY: no safety, this is undefined behavior
		// (i'd say compiler- and usage-specific, this can't produce any errors with the way the code is used now)
		// 
		// miri complains that this is UB, however i am yet to find a solution that:
		//   1. doesn't need unnecessary allocations
		//   2. doesn't require me to refactor everything else
		//   3. doesn't make the code look disgusting
		// as it stands right now, the safe rust code is uglier AND slower
		// and just as safe as this, provided the usage code remains the same.
		// (and the compiler doesn't do anything weird. and doesn't update)
		// 
		// isn't rust supposed to have "zero-cost" abstractions?
		// 
		// pull requests welcome, provided they at least somewhat satisfy the above
		// constraints.
		let refr: *mut PBTypeRef = unsafe {
			std::mem::transmute(refr)
		};
		if !(*refr).is_global || (*refr).reference == "Void" {
			return;
		}
		let with_correct_layer = Self::get_highest_layer(&*definition, &(*refr).reference, parent_layer)
			.expect("can't find highest layer, reference not resolved"); // trust the validator + resolver

		let highest_layer = Self::get_highest_layer(&*definition, &(*refr).reference, u32::MAX)
		.expect("can't find highest layer, reference not resolved"); // trust the validator + resolver

		(*refr).resolved_layer = Some(*with_correct_layer.get_layer());
		(*refr).is_highest_layer = *highest_layer.get_layer() == *with_correct_layer.get_layer();

		for generic_refr in &mut (*refr).generics {
			self.resolve_reference_unsafe(definition, generic_refr, parent_layer, tries + 1);
		}

		if let TypeOrCmdDef::TypeDef(tp) = with_correct_layer {
			if tp.get_attrs().contains_key("@resolve") && self.should_resolve_aliases {
				*refr = Self::resolve_alias(&(*refr), tp);
			}
		}
	}

	#[allow(unused)]
	fn resolve_references_unsafe(&self, definition: &mut PunybufDefinition) {
		for index in 0..definition.types.len() {
			let tp = &definition.types[index];
			let resolved_is_highest_layer = self.resolve_is_highest_layer(definition, tp.get_name().0, *tp.get_layer());

			match &mut definition.types[index] {
				PBTypeDef::Alias { is_highest_layer, .. } |
				PBTypeDef::Enum { is_highest_layer, .. } |
				PBTypeDef::Struct { is_highest_layer, .. } => {
					*is_highest_layer = resolved_is_highest_layer;
				}
			}

			match &definition.types[index] {
				PBTypeDef::Alias { alias, layer, .. } => {
					unsafe {self.resolve_reference_unsafe(definition, alias, *layer, 0); }
				}
				PBTypeDef::Enum { variants, layer, .. } => {
					for variant in variants {
						if let Some(val) = &variant.value {
							unsafe {self.resolve_reference_unsafe(definition, val, *layer, 0); }
						}
					}
				}
				PBTypeDef::Struct { fields, layer, .. } => {
					for field in fields {
						unsafe {self.resolve_reference_unsafe(definition, &field.value, *layer, 0); }
						if let Some(flags) = &field.flags {
							for flag in flags {
								let Some(val) = &flag.value else {continue};
								unsafe {self.resolve_reference_unsafe(definition, val, *layer, 0); }
							}
						}
					}
				}
			}
		}
		
		for index in 0..definition.commands.len() {
			let cmd = &definition.commands[index];
			let x = self.resolve_is_highest_layer(definition, &cmd.name, cmd.layer);

			definition.commands[index].is_highest_layer = x;

			let cmd = &definition.commands[index];
			match &cmd.argument {
				PBCommandArg::Ref(refr) => {
					unsafe { self.resolve_reference_unsafe(definition, refr, cmd.layer, 0); }
				}
				PBCommandArg::Struct { fields } => {
					for field in fields {
						unsafe { self.resolve_reference_unsafe(definition, &field.value, cmd.layer, 0); }
						if let Some(flags) = &field.flags {
							for flag in flags {
								let Some(val) = &flag.value else {continue};
								unsafe { self.resolve_reference_unsafe(definition, val, cmd.layer, 0); }
							}
						}
					}
				}
				PBCommandArg::None => {}
			}
			// TO DO: this code doesn't handle returns and errors
		}
	}
}

enum ResolvedReference {
	Resolved {
		resolved_layer: u32,
		is_highest_layer: bool,
		generics: VecDeque<Option<ResolvedReference>>,
	},
	Dealias(PBTypeRef),
}

struct ResolvedTypeDef {
	is_highest_layer: bool,
	data: ResolvedTypeDefData,
}

struct ResolvedField {
	refr: Option<ResolvedReference>,
	flags: Option<VecDeque<Option<ResolvedReference>>>
}

enum ResolvedTypeDefData {
	Alias {
		refr: Option<ResolvedReference>
	},
	Struct {
		fields: VecDeque<ResolvedField>
	},
	Enum {
		variants: VecDeque<Option<ResolvedReference>>
	},
}

enum ResolvedCommandArg {
	Ref(Option<ResolvedReference>),
	Struct {
		fields: VecDeque<ResolvedField>
	}
}

struct ResolvedCommand {
	arg: ResolvedCommandArg,
	ret: Option<ResolvedReference>,
	err: VecDeque<Option<ResolvedReference>>,
	is_highest_layer: bool,
}