use anyhow::Result;
use hashbrown::HashMap;
use mlua::{IntoLua, ObjectLike};
use yazi_binding::File;
use yazi_binding::elements::Line;
use yazi_core::tab::Folder;
use yazi_dds::spark::SparkKind;
use yazi_fs::{FilesSorter, FolderStage, SortBy};
use yazi_macro::{act, render, render_and, succ};
use yazi_parser::mgr::SortOpt;
use yazi_plugin::LUA;
use yazi_shared::{Source, data::Data, path::PathBufDyn};

use crate::{lives::Lives, Actor, Ctx};

pub struct Sort;

impl Sort {
	/// Compute linemode string values for all files in a folder by calling Lua
	fn compute_linemode_values(
		folder: &Folder,
		linemode: &str,
	) -> Result<HashMap<PathBufDyn, String>> {
		let mut values = HashMap::new();

		// Get the Linemode table from Lua
		let linemode_table: mlua::Table = LUA.globals().raw_get("Linemode")?;

		for file in folder.files.iter() {
			// Create a Linemode instance for this file
			let file_ud = File::new(file.clone()).into_lua(&LUA)?;
			let linemode_instance: mlua::Table = linemode_table.call_method("new", file_ud)?;

			// Call the linemode function (e.g., Linemode:size(), Linemode:duration(), etc.)
			if let Ok(result) = linemode_instance.call_method::<mlua::Value>(linemode, ()) {
				// Extract the string value from the result
				let value_str = match result {
					mlua::Value::String(s) => s.to_str()?.to_string(),
					mlua::Value::Table(t) => {
						// For ui.Line objects, try to extract text
						if let Ok(s) = t.call_method::<String>("__tostring", ()) {
							s
						} else {
							format!("{:?}", t)
						}
					}
					mlua::Value::UserData(ud) => {
						// Handle ui.Line UserData by extracting plain text from spans
						if let Ok(line) = ud.borrow::<Line>() {
							line.iter().map(|span| &*span.content).collect::<String>()
						} else {
							String::new()
						}
					}
					mlua::Value::Number(n) => n.to_string(),
					mlua::Value::Integer(i) => i.to_string(),
					_ => String::new(),
				};

				if !value_str.is_empty() {
					values.insert(file.urn().into(), value_str);
				}
			}
		}

		Ok(values)
	}
}

impl Actor for Sort {
	type Options = SortOpt;

	const NAME: &str = "sort";

	fn act(cx: &mut Ctx, opt: Self::Options) -> Result<Data> {
		let pref = &mut cx.tab_mut().pref;
		pref.sort_by = opt.by.unwrap_or(pref.sort_by);
		pref.sort_reverse = opt.reverse.unwrap_or(pref.sort_reverse);
		pref.sort_dir_first = opt.dir_first.unwrap_or(pref.sort_dir_first);
		pref.sort_sensitive = opt.sensitive.unwrap_or(pref.sort_sensitive);
		pref.sort_translit = opt.translit.unwrap_or(pref.sort_translit);

		let sorter = FilesSorter::from(&*pref);
		// If sorting by linemode, compute the linemode values first
		if sorter.by == SortBy::Linemode {
			// Compute linemode values for current, parent, and hovered folders
			let values_current = Lives::scope(&cx.core, || {
				Ok(Self::compute_linemode_values(&cx.current(), &sorter.linemode)
					.unwrap_or_else(|_| HashMap::new()))
			})?;

			let values_parent = if cx.parent().is_some() {
				Lives::scope(&cx.core, || {
					Ok(Self::compute_linemode_values(cx.parent().unwrap(), &sorter.linemode)
						.unwrap_or_else(|_| HashMap::new()))
				})?
			} else {
				HashMap::new()
			};

			let values_hovered = if cx.hovered_folder().is_some() {
				Lives::scope(&cx.core, || {
					Ok(Self::compute_linemode_values(cx.hovered_folder().unwrap(), &sorter.linemode)
						.unwrap_or_else(|_| HashMap::new()))
				})?
			} else {
				HashMap::new()
			};

			// Update the caches
			cx.current_mut().files.update_linemode_strings(values_current);
			if let Some(parent) = cx.parent_mut() {
				parent.files.update_linemode_strings(values_parent);
			}
			if let Some(hovered) = cx.hovered_folder_mut() {
				hovered.files.update_linemode_strings(values_hovered);
			}
		}

		let hovered = cx.hovered().map(|f| f.urn().to_owned());
		let apply = |f: &mut Folder| {
			if f.stage == FolderStage::Loading {
				render!();
				false
			} else {
				f.files.set_sorter(sorter.clone());
				render_and!(f.files.catchup_revision())
			}
		};

		// Apply to CWD and parent
		if let (a, Some(b)) = (apply(cx.current_mut()), cx.parent_mut().map(apply))
			&& (a | b)
		{
			act!(mgr:hover, cx)?;
			act!(mgr:update_paged, cx)?;
			cx.tasks.prework_sorted(&cx.mgr.tabs[cx.tab].current.files);
		}

		// Apply to hovered
		if let Some(h) = cx.hovered_folder_mut()
			&& apply(h)
		{
			render!(h.repos(None));
			act!(mgr:peek, cx, true)?;
		} else if cx.hovered().map(|f| f.urn()) != hovered.as_ref().map(Into::into) {
			act!(mgr:peek, cx)?;
			act!(mgr:watch, cx)?;
		}

		succ!();
	}

	fn hook(cx: &Ctx, _: &Self::Options) -> Option<SparkKind> {
		match cx.source() {
			Source::Ind => Some(SparkKind::IndSort),
			Source::Key => Some(SparkKind::KeySort),
			_ => None,
		}
	}
}
