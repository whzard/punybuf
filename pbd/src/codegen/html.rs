use std::collections::HashSet;

use crate::flattener::{PBCommandArg, PBCommandDef, PBEnumVariant, PBField, PBTypeDef, PBTypeRef, PunybufDefinition};

const DEFAULT_TEMPLATE: &str = include_str!("../../baked/template.html");

pub struct HTMLCodegen<'def> {
	definition: &'def PunybufDefinition,
	template: &'def str,
	buffer: String,
}

macro_rules! appendf {
	($s:ident, $x:literal, $($arg:tt)*) => {
		$s.buffer.push_str(&format!($x, $($arg)*))
	};
	($s:ident, $x:literal) => {
		$s.buffer.push_str(&format!($x))
	};
}

fn find_start_at(slice: &str, at: usize, pat: &str) -> Option<usize> {
    slice[at..].find(pat).map(|i| at + i)
}

impl<'d> HTMLCodegen<'d> {
	pub fn new(def: &'d PunybufDefinition, template: Option<&'d str>) -> Self {
		Self {
			definition: def,
			template: template.unwrap_or(DEFAULT_TEMPLATE),
			buffer: String::new()
		}
	}
	fn md_options(&mut self) -> markdown::Options {
		markdown::Options {
			..Default::default()
		}
	}
	fn transform_links(&mut self, mut s: String) -> String {
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
	fn is_primitive(&mut self, tp: &PBTypeDef) -> bool {
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
	fn gen_sidebar(&mut self) {
		appendf!(self, r#"<div class="sidebar-section">"#);
		appendf!(self, r#"<h3 class="sidebar-section-title">"#);
		appendf!(self, r#"Commands"#);
		appendf!(self, r#"</h3>"#);
		let mut seen_commands = HashSet::<&str>::new();
		for cmd in &self.definition.commands {
			if seen_commands.contains(&cmd.name.as_str()) { continue }
			appendf!(self,
				r##"<a class="sidebar-nav code" href="#{name}">{name}</a>"##,
				name = &cmd.name
			);
			seen_commands.insert(&cmd.name);
		}
		appendf!(self, r#"</div>"#);

		appendf!(self, r#"<div class="sidebar-section">"#);
		appendf!(self, r#"<h3 class="sidebar-section-title">"#);
		appendf!(self, r#"Types"#);
		appendf!(self, r#"</h3>"#);
		let mut seen_types = HashSet::<&str>::new();
		for tp in &self.definition.types {
			if self.is_primitive(tp) { continue }
			if seen_types.contains(&tp.get_name().0) { continue }
			appendf!(self,
				r##"<a class="sidebar-nav code" href="#{name}">{name}</a>"##,
				name = tp.get_name().0
			);
			seen_types.insert(tp.get_name().0);
		}
		appendf!(self, r#"</div>"#);

		appendf!(self, r#"<div class="sidebar-section">"#);
		appendf!(self, r#"<h3 class="sidebar-section-title">"#);
		appendf!(self, r#"Primitive types"#);
		appendf!(self, r#"</h3>"#);
		for tp in &self.definition.types {
			if !self.is_primitive(tp) { continue }
			if seen_types.contains(&tp.get_name().0) { continue }
			appendf!(self,
				r##"<a class="sidebar-nav code" href="#{name}">{name}</a>"##,
				name = tp.get_name().0
			);
			seen_types.insert(tp.get_name().0);
		}
		appendf!(self, r#"</div>"#);
	}
	fn gen_ref(&mut self, rf: &PBTypeRef) {
		if !rf.is_global {
			appendf!(self, r##"<span class="code">{name}</span>"##,
				name = rf.reference
			);
			return;
		}
		let link = if rf.is_highest_layer || rf.reference == "Void" {
			&rf.reference
		} else {
			&format!("{}-layer-{}", rf.reference, rf.resolved_layer.expect("layer not resolved"))
		};
		appendf!(self, r##"<a class="code" href="#{link}">{name}</a>"##,
			name = rf.reference
		);
		if !rf.generics.is_empty() {
			appendf!(self, r##"&lt;"##);
			for (i, param) in rf.generics.iter().enumerate() {
				if i != 0 {
					appendf!(self, ", ");
				}
				self.gen_ref(param);
			}
			appendf!(self, r##"&gt;"##);
		}
		if !rf.is_highest_layer && rf.reference != "Void" {
			appendf!(self, r##" (#{})"##, rf.resolved_layer.unwrap());
		}
	}
	fn gen_fields_table(&mut self, fields: &Vec<PBField>) {
		appendf!(self, r##"<table class="spec struct">"##);
		appendf!(self, r##"  <tbody>"##);
		for field in fields {
			if !field.attrs.is_empty() {
				appendf!(self, r##"    <tr class="attr-list">"##);
				appendf!(self, r##"      <td colspan="2">"##);
				for (attr, val) in &field.attrs {
					appendf!(self, r##"<span class="attr code">{}"##, attr);
					if let Some(val) = val {
						appendf!(self, r##"({val})"##)
					}
					appendf!(self, r##"</span>"##);
				}
				appendf!(self, r##"      </td>"##);
				appendf!(self, r##"    </tr>"##);
			}
			let name_begins_with_number = field.name.chars().nth(0).unwrap().is_numeric();
			appendf!(self, r##"    <tr>"##);
			appendf!(self, r##"      <td class="code">"##);
			appendf!(self, r##"        {}{}"##,
				if name_begins_with_number {
					r##"<span class="flag-mark">(flags)</span>"##
				} else {
					&field.name
				},
				if field.flags.is_some() && !name_begins_with_number {
					r##"<span class="flag-mark">.</span>"##
				} else { "" }
			);
			appendf!(self, r##"      </td>"##);
			appendf!(self, r##"      <td class="code">"##);
			appendf!(self, r##"        "##);
			self.gen_ref(&field.value);
			appendf!(self, r##"      </td>"##);
			appendf!(self, r##"    </tr>"##);
			if !field.doc.is_empty() {
				appendf!(self, r##"    <tr class="mini-item-description">"##);
				let doc = markdown::to_html_with_options(&field.doc, &self.md_options()).unwrap();
				let doc = self.transform_links(doc);
				appendf!(self, r##"      <td colspan="2" class="md">{doc}</div>"##);
				appendf!(self, r##"    </tr>"##);
			}
			let Some(flags) = &field.flags else { continue };
			for flag in flags {
				if !flag.attrs.is_empty() {
					appendf!(self, r##"    <tr class="flag attr-list">"##);
					appendf!(self, r##"      <td colspan="2">"##);
					for (attr, val) in &flag.attrs {
						appendf!(self, r##"<span class="attr code">{}"##, attr);
						if let Some(val) = val {
							appendf!(self, r##"({val})"##)
						}
						appendf!(self, r##"</span>"##);
					}
					appendf!(self, r##"      </td>"##);
					appendf!(self, r##"    </tr>"##);
				}
				appendf!(self, r##"    <tr class="flag">"##);
				appendf!(self, r##"      <td class="code">"##);
				appendf!(self, r##"        {}<span class="flag-mark">?</span>"##, flag.name);
				appendf!(self, r##"      </td>"##);
				appendf!(self, r##"      <td class="code">"##);
				if let Some(v) = &flag.value {
					appendf!(self, r##"        "##);
					self.gen_ref(v);
				}
				appendf!(self, r##"      </td>"##);
				appendf!(self, r##"    </tr>"##);
				if !flag.doc.is_empty() {
					appendf!(self, r##"    <tr class="flag mini-item-description">"##);
					let doc = markdown::to_html_with_options(&flag.doc, &self.md_options()).unwrap();
					let doc = self.transform_links(doc);
					appendf!(self, r##"      <td colspan="2" class="md">{doc}</div>"##);
					appendf!(self, r##"    </tr>"##);
				}
			}
		}
		appendf!(self, r##"  </tbody>"##);
		appendf!(self, r##"</table>"##);
	}
	fn gen_attr(&mut self, attr: &str, value: &Option<String>) {
		appendf!(self, r##"<span class="attr code">{}"##, attr);
		if let Some(val) = value {
			appendf!(self, r##"({val})"##)
		}
		appendf!(self, r##"</span>"##);
	}
	fn gen_variants(&mut self, variants: &Vec<PBEnumVariant>) {
		for variant in variants {
			if !variant.attrs.is_empty() {
				appendf!(self, r##"    <tr class="attr-list">"##);
				appendf!(self, r##"      <td colspan="2">"##);
				for (attr, val) in &variant.attrs {
					self.gen_attr(attr, val);
				}
				appendf!(self, r##"      </td>"##);
				appendf!(self, r##"    </tr>"##);
			}
			appendf!(self, r##"    <tr>"##);
			appendf!(self, r##"      <td class="code">"##);
			appendf!(self, r##"        {}"##, variant.name);
			appendf!(self, r##"      </td>"##);
			appendf!(self, r##"      <td class="code">"##);
			appendf!(self, r##"        "##);
			variant.value.as_ref().map(|r| self.gen_ref(r));
			appendf!(self, r##"      </td>"##);
			appendf!(self, r##"    </tr>"##);
			if !variant.doc.is_empty() {
				appendf!(self, r##"    <tr class="mini-item-description">"##);
				let doc = markdown::to_html_with_options(&variant.doc, &self.md_options()).unwrap();
				let doc = self.transform_links(doc);
				appendf!(self, r##"      <td colspan="2" class="md">{doc}</div>"##);
				appendf!(self, r##"    </tr>"##);
			}
		}
	}
	fn gen_command(&mut self, cmd: &PBCommandDef) {
		if !cmd.is_highest_layer {
			appendf!(self, r##"<details class="layer">"##);
			appendf!(self, r##"<summary><div>"##);
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
		appendf!(self, r##"<{h} class="item-header" id="{link}">
			{name}
			<span class="chip code">#{id}</span> {chip}
		</{h}>"##, name = cmd.name, id = cmd.command_id);
		if !cmd.is_highest_layer {
			appendf!(self, r##"</div></summary>"##);
		}
		appendf!(self, r##"<div class="item-content">"##);
		if !cmd.attrs.is_empty() {
			appendf!(self, r##"<div class="item-attr-list">"##);
			for (attr, val) in &cmd.attrs {
				self.gen_attr(attr, val);
			}
			appendf!(self, r##"</div>"##);
		}
		if !cmd.doc.is_empty() {
			let doc = markdown::to_html_with_options(&cmd.doc, &self.md_options()).unwrap();
			let doc = self.transform_links(doc);
			appendf!(self, r##"<div class="md description">{doc}</div>"##);
		}
		match &cmd.argument {
			PBCommandArg::None => {},
			PBCommandArg::Ref(rf) => {
				appendf!(self, r##"<h4>Argument</h4>"##);
				appendf!(self, r##"<span class="code">"##);
				self.gen_ref(rf);
				appendf!(self, r##"</span>"##);
			},
			PBCommandArg::Struct { fields } => {
				appendf!(self, r##"<h4>Argument</h4>"##);
				self.gen_fields_table(fields);
			},
		}
		appendf!(self, r##"<h4>Return value</h4>"##);
		appendf!(self, r##"<span>&RightArrow; <span class="code">"##);
		self.gen_ref(&cmd.ret);
		appendf!(self, r##"</span></span>"##);
		if cmd.ret.reference != "Void" {
			appendf!(self, r##"<h4>Errors</h4>"##);
			appendf!(self, r##"<table class="spec enum">"##);
			appendf!(self, r##"  <tbody>"##);
			appendf!(self, r##"    <tr>"##);
			appendf!(self, r##"      <td class="code default-error">"##);
			appendf!(self, r##"        (UnexpectedError)"##);
			appendf!(self, r##"      </td>"##);
			appendf!(self, r##"      <td class="code">"##);
			appendf!(self, r##"        <a href="#String">String</a>"##);
			appendf!(self, r##"      </td>"##);
			appendf!(self, r##"    </tr>"##);
			self.gen_variants(&cmd.err);
			appendf!(self, r##"  </tbody>"##);
			appendf!(self, r##"</table>"##);
		}
		appendf!(self, r##"</div>"##);
		if !cmd.is_highest_layer {
			appendf!(self, r##"</details>"##);
		}
	}
	fn gen_type(&mut self, tp: &PBTypeDef) {
		if !tp.is_highest_layer() {
			appendf!(self, r##"<details class="layer">"##);
			appendf!(self, r##"<summary><div>"##);
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
			appendf!(self,
				r##"<{h} class="item-header" id="{link}">{name} {chip}</{h}>"##,
				name = tp.get_name().0
			);
		} else {
			appendf!(self,
				r##"<{h} class="item-header" id="{link}">{name}
				<span class="generic-args code">&lt;{g}&gt;</span> {chip}</{h}>"##,
				name = tp.get_name().0,
				g = self.generics(tp.get_generics().0)
			);
		}
		if !tp.is_highest_layer() {
			appendf!(self, r##"</div></summary>"##);
		}
		appendf!(self, r##"<div class="item-content">"##);
		if !tp.get_attrs().is_empty() {
			appendf!(self, r##"<div class="item-attr-list">"##);
			for (attr, val) in tp.get_attrs() {
				self.gen_attr(attr, val);
			}
			appendf!(self, r##"</div>"##);
		}
		if !tp.get_doc().is_empty() {
			let doc = markdown::to_html_with_options(&tp.get_doc(), &self.md_options()).unwrap();
			let doc = self.transform_links(doc);
			appendf!(self, r##"<div class="md description">{doc}</div>"##);
		}
		if tp.get_attrs().contains_key("@builtin") {
			appendf!(self, r##"</div>"##);
			appendf!(
				self,
				r##"<span class="notice md">&#9432; This type is <code>@builtin</code>.</span>"##
			);
			return;
		}
		match tp {
			PBTypeDef::Struct { fields, .. } => {
				self.gen_fields_table(fields);
			},
			PBTypeDef::Enum { variants, .. } => {
				appendf!(self, r##"<table class="spec enum">"##);
				appendf!(self, r##"  <tbody>"##);
				self.gen_variants(variants);
				appendf!(self, r##"  </tbody>"##);
				appendf!(self, r##"</table>"##);
			},
			PBTypeDef::Alias { alias, .. } => {
				appendf!(self, r##"<h4>Alias</h4>"##);
				self.gen_ref(alias);
			},
		}
		appendf!(self, r##"</div>"##);
		if !tp.is_highest_layer() {
			appendf!(self, r##"</details>"##);
		}
	}
	fn gen_main(&mut self) {
		appendf!(self, "<h1>Commands</h1>");
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
			self.gen_command(cmd);
			let lower_layer = self.definition.commands
				.iter()
				.filter(|c| c.name == cmd.name && !c.is_highest_layer)
				.rev()
				.collect::<Vec<_>>();
			if !lower_layer.is_empty() {
				appendf!(self,
					r##"<p class="notice">&#9432; This command is also defined on other layers</p>"##
				);
			}
			for cmd in lower_layer {
				self.gen_command(cmd);
			}
		}
		appendf!(self, "<h1>Types</h1>");
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
			self.gen_type(tp);
			let lower_layer = self.definition.types
				.iter()
				.filter(|t| t.get_name().0 == tp.get_name().0 && !t.is_highest_layer())
				.rev()
				.collect::<Vec<_>>();
			if !lower_layer.is_empty() {
				appendf!(self,
					r##"<p class="notice">&#9432; This type is also defined on other layers</p>"##
				);
			}
			for tp in lower_layer {
				self.gen_type(tp);
			}
		}
		appendf!(self, "<h1>Primitive types</h1>");
		for tp in &self.definition.types {
			if !self.is_primitive(tp) { continue }
			if !tp.is_highest_layer() { continue }
			self.gen_type(tp);
			let lower_layer = self.definition.types
				.iter()
				.filter(|t| t.get_name().0 == tp.get_name().0 && !t.is_highest_layer())
				.rev()
				.collect::<Vec<_>>();
			if !lower_layer.is_empty() {
				appendf!(self,
					r##"<p class="notice">&#9432; This type is also defined on other layers</p>"##
				);
			}
			for tp in lower_layer {
				self.gen_type(tp);
			}
		}
	}
	pub fn codegen(&mut self) -> String {
		self.gen_sidebar();
		let template = self.template;
		let template = template.replace("%sidebar", &self.buffer);
		self.buffer = String::new();
		self.gen_main();
		let template = template.replace("%main", &self.buffer);
		template
	}
}