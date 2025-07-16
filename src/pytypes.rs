// SPDX-FileCopyrightText: Copyright DB InfraGO AG
// SPDX-License-Identifier: Apache-2.0

use capellambse_macros::PyWrapper;
use pyo3::{
    exceptions::PyTypeError,
    intern,
    prelude::*,
    sync::GILOnceCell,
    types::{PyString, PyType},
};

/// A PyAny that has been type-checked to be an AwesomeVersion object.
#[derive(PyWrapper)]
pub struct AwesomeVersion(Py<PyAny>);

impl AwesomeVersion {
    #[inline]
    pub fn cls<'py>(py: Python<'py>) -> PyResult<Bound<'py, PyType>> {
        static CELL: GILOnceCell<Py<PyType>> = GILOnceCell::new();
        CELL.get_or_try_init(py, || {
            Ok(py
                .import(intern!(py, "awesomeversion"))?
                .getattr(intern!(py, "AwesomeVersion"))?
                .downcast_into()?
                .unbind())
        })
        .map(|cls| cls.bind(py).clone())
    }

    pub fn new(v: &Bound<'_, PyString>) -> PyResult<Self> {
        Ok(Self(Self::cls(v.py())?.call1((v,))?.unbind()))
    }

    pub fn le(&self, py: Python<'_>, other: &Self) -> PyResult<bool> {
        self.0.bind(py).le(&other.0)
    }

    pub fn ge(&self, py: Python<'_>, other: &Self) -> PyResult<bool> {
        self.0.bind(py).ge(&other.0)
    }
}

impl<'py> FromPyObject<'py> for AwesomeVersion {
    fn extract_bound(ob: &Bound<'py, PyAny>) -> PyResult<Self> {
        let py = ob.py();
        if let Ok(ob) = ob.downcast::<PyString>() {
            Self::new(ob)
        } else if ob.is_instance(Self::cls(py)?.as_any()).unwrap_or(false) {
            Ok(Self(ob.clone().unbind()))
        } else {
            Err(PyTypeError::new_err(
                "Expected a str with a version number or an AwesomeVersion object",
            ))
        }
    }
}
