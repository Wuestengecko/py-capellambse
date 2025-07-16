// SPDX-FileCopyrightText: Copyright DB InfraGO AG
// SPDX-License-Identifier: Apache-2.0

use pyo3::prelude::*;

mod exs;
mod model;
mod namespace;
mod parse;
mod pytypes;

#[pymodule(name = "_compiled", gil_used = false)]
fn setup_module(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(exs::serialize, m)?)?;
    model::setup(m)?;
    namespace::setup(m)?;
    pytypes::setup(m)?;

    Ok(())
}
