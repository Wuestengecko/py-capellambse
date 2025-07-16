// SPDX-FileCopyrightText: Copyright DB InfraGO AG
// SPDX-License-Identifier: Apache-2.0

use std::{convert::Infallible, ops::Deref};

use pyo3::exceptions::PyTypeError;
use pyo3::intern;
use pyo3::prelude::*;
use pyo3::sync::GILOnceCell;
use pyo3::types::{PyString, PyType};

/// A PyAny that has been type-checked to be an AwesomeVersion object.
#[derive(IntoPyObject)]
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

    #[inline]
    pub fn clone_ref(&self, py: Python<'_>) -> Self {
        Self(self.0.clone_ref(py))
    }

    #[inline]
    pub fn drop_ref(self, py: Python<'_>) {
        self.0.drop_ref(py);
    }

    pub fn new(v: &Bound<'_, PyString>) -> PyResult<Self> {
        Ok(Self(Self::cls(v.py())?.call1((v,))?.unbind()))
    }

    pub fn into_inner(self) -> Py<PyAny> {
        self.0
    }

    pub fn as_inner(&self) -> &Py<PyAny> {
        &self.0
    }

    pub fn le(&self, py: Python<'_>, other: &Self) -> PyResult<bool> {
        self.0.bind(py).le(other)
    }

    pub fn ge(&self, py: Python<'_>, other: &Self) -> PyResult<bool> {
        self.0.bind(py).ge(other)
    }
}

impl Into<Py<PyAny>> for AwesomeVersion {
    fn into(self) -> Py<PyAny> {
        self.0
    }
}

impl Deref for AwesomeVersion {
    type Target = Py<PyAny>;

    fn deref(&self) -> &Self::Target {
        &self.0
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

impl<'py> IntoPyObject<'py> for &'py AwesomeVersion {
    type Target = PyAny;
    type Output = Bound<'py, Self::Target>;
    type Error = Infallible;

    fn into_pyobject(self, py: Python<'py>) -> Result<Self::Output, Self::Error> {
        Ok(self.0.bind(py).clone())
    }
}
