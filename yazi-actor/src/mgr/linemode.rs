use anyhow::Result;
use yazi_fs::SortBy;
use yazi_macro::{act, render, succ};
use yazi_parser::mgr::LinemodeOpt;
use yazi_shared::data::Data;

use crate::{Actor, Ctx};

pub struct Linemode;

impl Actor for Linemode {
	type Options = LinemodeOpt;

	const NAME: &str = "linemode";

	fn act(cx: &mut Ctx, opt: Self::Options) -> Result<Data> {
		let mut needs_resort = false;

		{
			let tab = cx.tab_mut();
			if opt.new != tab.pref.linemode {
				tab.pref.linemode = opt.new.into_owned();
				render!();

				needs_resort = tab.pref.sort_by == SortBy::Linemode;
			}
		}

		if needs_resort {
			act!(mgr:sort, cx)?;
		}

		succ!();
	}
}
