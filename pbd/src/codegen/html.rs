use std::collections::HashSet;

use crate::flattener::{PBCommandArg, PBCommandDef, PBEnumVariant, PBField, PBTypeDef, PBTypeRef, PunybufDefinition};

const DEFAULT_TEMPLATE: &str = include_str!("../../baked/template.html");

pub struct HTMLCodegen<'def> {
	definition: &'def PunybufDefinition,
	template: &'def str,
}

macro_rules! appendf {
	($s:ident, $x:literal, $($arg:tt)*) => {
		$s.push_str(&format!($x, $($arg)*))
	};
	($s:ident, $x:literal) => {
		$s.push_str(&format!($x))
	};
}

fn find_start_at(slice: &str, at: usize, pat: &str) -> Option<usize> {
    slice[at..].find(pat).map(|i| at + i)
}

impl<'d> HTMLCodegen<'d> {
	pub fn new(def: &'d PunybufDefinition, template: Option<&'d str>) -> Self {
		Self {
			definition: def,
			template: template.unwrap_or(DEFAULT_TEMPLATE)
		}
	}
	fn md_options(&self) -> markdown::Options {
		markdown::Options {
			..Default::default()
		}
	}
	fn transform_links(&self, mut s: String) -> String {
		const PATTERN: &str = r#"href=""#;
		let mut last_position = 0;
		while let Some(index) = find_start_at(&s, last_position, PATTERN) {
			let index = index + PATTERN.len();
			last_position = index;
			if
				s[index..].starts_with("http://") ||
				s[index..].starts_with("https://") ||
				s[index..].starts_with("file://") ||
				s[index..].starts_with("#")
			{
				continue;
			}
			s.insert_str(index, "#");
			last_position += 1;
		}
		s
	}
	fn is_primitive(&self, tp: &PBTypeDef) -> bool {
		return (
			self.definition.includes_common &&
			matches!(
				tp.get_name().0,
				"Map" | "KeyPair" | "Done" | "Boolean" | "Optional"
			) 
		) || tp.get_attrs().contains_key("@builtin");
	}
	fn generics(&self, g: &Vec<String>) -> String {
		g.iter()
			.map(|g| {
				g.as_str()
			})
			.collect::<Vec<_>>()
			.join(", ")
	}
	fn gen_sidebar(&self) -> String {
		let mut result = String::new();
		appendf!(result, r#"<div class="sidebar-section">"#);
		appendf!(result, r#"<h3 class="sidebar-section-title">"#);
		appendf!(result, r#"Commands"#);
		appendf!(result, r#"</h3>"#);
		let mut seen_commands = HashSet::<&str>::new();
		for cmd in &self.definition.commands {
			if seen_commands.contains(&cmd.name.as_str()) { continue }
			appendf!(result,
				r##"<a class="sidebar-nav code" href="#{name}">{name}</a>"##,
				name = &cmd.name
			);
			seen_commands.insert(&cmd.name);
		}
		appendf!(result, r#"</div>"#);

		appendf!(result, r#"<div class="sidebar-section">"#);
		appendf!(result, r#"<h3 class="sidebar-section-title">"#);
		appendf!(result, r#"Types"#);
		appendf!(result, r#"</h3>"#);
		let mut seen_types = HashSet::<&str>::new();
		for tp in &self.definition.types {
			if self.is_primitive(tp) { continue }
			if seen_types.contains(&tp.get_name().0) { continue }
			appendf!(result,
				r##"<a class="sidebar-nav code" href="#{name}">{name}</a>"##,
				name = tp.get_name().0
			);
			seen_types.insert(tp.get_name().0);
		}
		appendf!(result, r#"</div>"#);

		appendf!(result, r#"<div class="sidebar-section">"#);
		appendf!(result, r#"<h3 class="sidebar-section-title">"#);
		appendf!(result, r#"Primitive types"#);
		appendf!(result, r#"</h3>"#);
		for tp in &self.definition.types {
			if !self.is_primitive(tp) { continue }
			if seen_types.contains(&tp.get_name().0) { continue }
			appendf!(result,
				r##"<a class="sidebar-nav code" href="#{name}">{name}</a>"##,
				name = tp.get_name().0
			);
			seen_types.insert(tp.get_name().0);
		}
		appendf!(result, r#"</div>"#);
		result
	}
	fn gen_ref(&self, rf: &PBTypeRef) -> String {
		let mut result = String::new();
		if !rf.is_global {
			appendf!(result, r##"<span class="code">{name}</span>"##,
				name = rf.reference
			);
			return result;
		}
		let link = if rf.is_highest_layer || rf.reference == "Void" {
			&rf.reference
		} else {
			&format!("{}-layer-{}", rf.reference, rf.resolved_layer.expect("layer not resolved"))
		};
		appendf!(result, r##"<a class="code" href="#{link}">{name}</a>"##,
			name = rf.reference
		);
		if !rf.generics.is_empty() {
			appendf!(result, r##"&lt;"##);
			for (i, param) in rf.generics.iter().enumerate() {
				if i != 0 {
					appendf!(result, ", ");
				}
				result.push_str(&self.gen_ref(param));
			}
			appendf!(result, r##"&gt;"##);
		}
		if !rf.is_highest_layer && rf.reference != "Void" {
			appendf!(result, r##" (#{})"##, rf.resolved_layer.unwrap());
		}
		result
	}
	fn gen_fields_table(&self, fields: &Vec<PBField>) -> String {
		let mut result = String::new();
		appendf!(result, r##"<table class="spec struct">"##);
		appendf!(result, r##"  <tbody>"##);
		for field in fields {
			if !field.attrs.is_empty() {
				appendf!(result, r##"    <tr class="attr-list">"##);
				appendf!(result, r##"      <td colspan="2">"##);
				for (attr, val) in &field.attrs {
					appendf!(result, r##"<span class="attr code">{}"##, attr);
					if let Some(val) = val {
						appendf!(result, r##"({val})"##)
					}
					appendf!(result, r##"</span>"##);
				}
				appendf!(result, r##"      </td>"##);
				appendf!(result, r##"    </tr>"##);
			}
			let name_begins_with_number = field.name.chars().nth(0).unwrap().is_numeric();
			appendf!(result, r##"    <tr>"##);
			appendf!(result, r##"      <td class="code">"##);
			appendf!(result, r##"        {}{}"##,
				if name_begins_with_number {
					r##"<span class="flag-mark">(flags)</span>"##
				} else {
					&field.name
				},
				if field.flags.is_some() && !name_begins_with_number {
					r##"<span class="flag-mark">.</span>"##
				} else { "" }
			);
			appendf!(result, r##"      </td>"##);
			appendf!(result, r##"      <td class="code">"##);
			appendf!(result, r##"        {}"##, self.gen_ref(&field.value));
			appendf!(result, r##"      </td>"##);
			appendf!(result, r##"    </tr>"##);
			if !field.doc.is_empty() {
				appendf!(result, r##"    <tr class="mini-item-description">"##);
				let doc = markdown::to_html_with_options(&field.doc, &self.md_options()).unwrap();
				let doc = self.transform_links(doc);
				appendf!(result, r##"      <td colspan="2" class="md">{doc}</div>"##);
				appendf!(result, r##"    </tr>"##);
			}
			let Some(flags) = &field.flags else { continue };
			for flag in flags {
				if !flag.attrs.is_empty() {
					appendf!(result, r##"    <tr class="flag attr-list">"##);
					appendf!(result, r##"      <td colspan="2">"##);
					for (attr, val) in &flag.attrs {
						appendf!(result, r##"<span class="attr code">{}"##, attr);
						if let Some(val) = val {
							appendf!(result, r##"({val})"##)
						}
						appendf!(result, r##"</span>"##);
					}
					appendf!(result, r##"      </td>"##);
					appendf!(result, r##"    </tr>"##);
				}
				appendf!(result, r##"    <tr class="flag">"##);
				appendf!(result, r##"      <td class="code">"##);
				appendf!(result, r##"        {}<span class="flag-mark">?</span>"##, flag.name);
				appendf!(result, r##"      </td>"##);
				appendf!(result, r##"      <td class="code">"##);
				if let Some(v) = &flag.value {
					appendf!(result, r##"        {}"##, self.gen_ref(v));
				}
				appendf!(result, r##"      </td>"##);
				appendf!(result, r##"    </tr>"##);
				if !flag.doc.is_empty() {
					appendf!(result, r##"    <tr class="flag mini-item-description">"##);
					let doc = markdown::to_html_with_options(&flag.doc, &self.md_options()).unwrap();
					let doc = self.transform_links(doc);
					appendf!(result, r##"      <td colspan="2" class="md">{doc}</div>"##);
					appendf!(result, r##"    </tr>"##);
				}
			}
		}
		appendf!(result, r##"  </tbody>"##);
		appendf!(result, r##"</table>"##);
		result
	}
	fn gen_attr(&self, attr: &str, value: &Option<String>) -> String {
		let mut result = String::new();
		appendf!(result, r##"<span class="attr code">{}"##, attr);
		if let Some(val) = value {
			appendf!(result, r##"({val})"##)
		}
		appendf!(result, r##"</span>"##);
		result
	}
	fn gen_variants(&self, variants: &Vec<PBEnumVariant>) -> String {
		let mut result = String::new();
		for variant in variants {
			if !variant.attrs.is_empty() {
				appendf!(result, r##"    <tr class="attr-list">"##);
				appendf!(result, r##"      <td colspan="2">"##);
				for (attr, val) in &variant.attrs {
					result.push_str(&self.gen_attr(attr, val));
				}
				appendf!(result, r##"      </td>"##);
				appendf!(result, r##"    </tr>"##);
			}
			appendf!(result, r##"    <tr>"##);
			appendf!(result, r##"      <td class="code">"##);
			appendf!(result, r##"        {}"##, variant.name);
			appendf!(result, r##"      </td>"##);
			appendf!(result, r##"      <td class="code">"##);
			appendf!(result, r##"        {}"##,
				variant.value.as_ref().map(|r| self.gen_ref(r)).unwrap_or("".into())
			);
			appendf!(result, r##"      </td>"##);
			appendf!(result, r##"    </tr>"##);
			if !variant.doc.is_empty() {
				appendf!(result, r##"    <tr class="mini-item-description">"##);
				let doc = markdown::to_html_with_options(&variant.doc, &self.md_options()).unwrap();
				let doc = self.transform_links(doc);
				appendf!(result, r##"      <td colspan="2" class="md">{doc}</div>"##);
				appendf!(result, r##"    </tr>"##);
			}
		}
		result
	}
	fn gen_command(&self, cmd: &PBCommandDef) -> String {
		let mut result = String::new();
		if !cmd.is_highest_layer {
			appendf!(result, r##"<details class="layer">"##);
			appendf!(result, r##"<summary><div>"##);
		}
		let link = if cmd.is_highest_layer {
			&cmd.name
		} else {
			&format!(r##"{}-layer-{}"##, cmd.name, cmd.layer)
		};
		let chip = if cmd.is_highest_layer {
			""
		} else {
			&format!(r##"<span class="chip code">Layer {}</span>"##, cmd.layer)
		};
		let h = if cmd.is_highest_layer {
			"h2"
		} else {
			"h3"
		};
		appendf!(result, r##"<{h} class="item-header" id="{link}">
			{name}
			<span class="chip code">#{id}</span> {chip}
		</{h}>"##, name = cmd.name, id = cmd.command_id);
		if !cmd.is_highest_layer {
			appendf!(result, r##"</div></summary>"##);
		}
		appendf!(result, r##"<div class="item-content">"##);
		if !cmd.attrs.is_empty() {
			appendf!(result, r##"<div class="item-attr-list">"##);
			for (attr, val) in &cmd.attrs {
				result.push_str(&self.gen_attr(attr, val));
			}
			appendf!(result, r##"</div>"##);
		}
		if !cmd.doc.is_empty() {
			let doc = markdown::to_html_with_options(&cmd.doc, &self.md_options()).unwrap();
			let doc = self.transform_links(doc);
			appendf!(result, r##"<div class="md description">{doc}</div>"##);
		}
		match &cmd.argument {
			PBCommandArg::None => {},
			PBCommandArg::Ref(rf) => {
				appendf!(result, r##"<h4>Argument</h4>"##);
				appendf!(result, r##"<span class="code">{}</span>"##, self.gen_ref(rf));
			},
			PBCommandArg::Struct { fields } => {
				appendf!(result, r##"<h4>Argument</h4>"##);
				result.push_str(&self.gen_fields_table(fields));
			},
		}
		appendf!(result, r##"<h4>Return value</h4>"##);
		appendf!(result, r##"<span>&RightArrow; <span class="code">{}</span></span>"##, self.gen_ref(&cmd.ret));
		if cmd.ret.reference != "Void" {
			appendf!(result, r##"<h4>Errors</h4>"##);
			appendf!(result, r##"<table class="spec enum">"##);
			appendf!(result, r##"  <tbody>"##);
			appendf!(result, r##"    <tr>"##);
			appendf!(result, r##"      <td class="code default-error">"##);
			appendf!(result, r##"        (UnexpectedError)"##);
			appendf!(result, r##"      </td>"##);
			appendf!(result, r##"      <td class="code">"##);
			appendf!(result, r##"        <a href="#String">String</a>"##);
			appendf!(result, r##"      </td>"##);
			appendf!(result, r##"    </tr>"##);
			result.push_str(&self.gen_variants(&cmd.err));
			appendf!(result, r##"  </tbody>"##);
			appendf!(result, r##"</table>"##);
		}
		appendf!(result, r##"</div>"##);
		if !cmd.is_highest_layer {
			appendf!(result, r##"</details>"##);
		}
		result
	}
	fn gen_type(&self, tp: &PBTypeDef) -> String {
		let mut result = String::new();
		if !tp.is_highest_layer() {
			appendf!(result, r##"<details class="layer">"##);
			appendf!(result, r##"<summary><div>"##);
		}
		let link = if tp.is_highest_layer() {
			tp.get_name().0
		} else {
			&format!(r##"{}-layer-{}"##, tp.get_name().0, tp.get_layer())
		};
		let chip = if tp.is_highest_layer() {
			""
		} else {
			&format!(r##"<span class="chip code">Layer {}</span>"##, tp.get_layer())
		};
		let h = if tp.is_highest_layer() {
			"h2"
		} else {
			"h3"
		};
		if tp.get_generics().0.is_empty() {
			appendf!(result,
				r##"<{h} class="item-header" id="{link}">{name} {chip}</{h}>"##,
				name = tp.get_name().0
			);
		} else {
			appendf!(result,
				r##"<{h} class="item-header" id="{link}">{name}
				<span class="generic-args code">&lt;{g}&gt;</span> {chip}</{h}>"##,
				name = tp.get_name().0,
				g = self.generics(tp.get_generics().0)
			);
		}
		if !tp.is_highest_layer() {
			appendf!(result, r##"</div></summary>"##);
		}
		appendf!(result, r##"<div class="item-content">"##);
		if !tp.get_attrs().is_empty() {
			appendf!(result, r##"<div class="item-attr-list">"##);
			for (attr, val) in tp.get_attrs() {
				result.push_str(&self.gen_attr(attr, val));
			}
			appendf!(result, r##"</div>"##);
		}
		if !tp.get_doc().is_empty() {
			let doc = markdown::to_html_with_options(&tp.get_doc(), &self.md_options()).unwrap();
			let doc = self.transform_links(doc);
			appendf!(result, r##"<div class="md description">{doc}</div>"##);
		}
		if tp.get_attrs().contains_key("@builtin") {
			appendf!(result, r##"</div>"##);
			appendf!(
				result,
				r##"<span class="notice md">&#9432; This type is <code>@builtin</code>.</span>"##
			);
			return result;
		}
		match tp {
			PBTypeDef::Struct { fields, .. } => {
				result.push_str(&self.gen_fields_table(fields));
			},
			PBTypeDef::Enum { variants, .. } => {
				appendf!(result, r##"<table class="spec enum">"##);
				appendf!(result, r##"  <tbody>"##);
				result.push_str(&self.gen_variants(variants));
				appendf!(result, r##"  </tbody>"##);
				appendf!(result, r##"</table>"##);
			},
			PBTypeDef::Alias { alias, .. } => {
				appendf!(result, r##"<h4>Alias</h4>"##);
				result.push_str(&self.gen_ref(alias));
			},
		}
		appendf!(result, r##"</div>"##);
		if !tp.is_highest_layer() {
			appendf!(result, r##"</details>"##);
		}
		result
	}
	fn gen_main(&self) -> String {
		let mut result = String::new();
		appendf!(result, "<h1>Commands</h1>");
		let mut seen_commands = HashSet::<&str>::new();
		for cmd in &self.definition.commands {
			if seen_commands.contains(&cmd.name.as_ref()) { continue }
			let cmd = if cmd.is_highest_layer { cmd } else {
				self.definition.commands
					.iter()
					.find(|c| c.name == cmd.name && c.is_highest_layer)
					.expect("command not found")
			};
			seen_commands.insert(&cmd.name);
			result.push_str(&self.gen_command(cmd));
			let lower_layer = self.definition.commands
				.iter()
				.filter(|c| c.name == cmd.name && !c.is_highest_layer)
				.rev()
				.collect::<Vec<_>>();
			if !lower_layer.is_empty() {
				appendf!(result,
					r##"<p class="notice">&#9432; This command is also defined on other layers</p>"##
				);
			}
			for cmd in lower_layer {
				result.push_str(&self.gen_command(cmd));
			}
		}
		appendf!(result, "<h1>Types</h1>");
		let mut seen_types = HashSet::new();
		for tp in &self.definition.types {
			if self.is_primitive(tp) { continue }
			if seen_types.contains(&tp.get_name().0) { continue }
			let tp = if tp.is_highest_layer() { tp } else {
				self.definition.types
					.iter()
					.find(|t| t.get_name().0 == tp.get_name().0 && t.is_highest_layer())
					.expect("command not found")
			};
			seen_types.insert(tp.get_name().0);
			result.push_str(&self.gen_type(tp));
			let lower_layer = self.definition.types
				.iter()
				.filter(|t| t.get_name().0 == tp.get_name().0 && !t.is_highest_layer())
				.rev()
				.collect::<Vec<_>>();
			if !lower_layer.is_empty() {
				appendf!(result,
					r##"<p class="notice">&#9432; This type is also defined on other layers</p>"##
				);
			}
			for tp in lower_layer {
				result.push_str(&self.gen_type(tp));
			}
		}
		appendf!(result, "<h1>Primitive types</h1>");
		for tp in &self.definition.types {
			if !self.is_primitive(tp) { continue }
			if !tp.is_highest_layer() { continue }
			result.push_str(&&self.gen_type(tp));
			let lower_layer = self.definition.types
				.iter()
				.filter(|t| t.get_name().0 == tp.get_name().0 && !t.is_highest_layer())
				.rev()
				.collect::<Vec<_>>();
			if !lower_layer.is_empty() {
				appendf!(result,
					r##"<p class="notice">&#9432; This type is also defined on other layers</p>"##
				);
			}
			for tp in lower_layer {
				result.push_str(&self.gen_type(tp));
			}
		}
		result
	}
	pub fn codegen(&self) -> String {
		self.template
			.replace("%sidebar", &self.gen_sidebar())
			.replace("%main", &self.gen_main())
	}
}