// SPDX-FileCopyrightText: Copyright DB InfraGO AG
// SPDX-License-Identifier: Apache-2.0

use std::ops::Deref;

use capellambse_macros::PyWrapper;
use pyo3::{
    exceptions::PyTypeError,
    intern,
    prelude::*,
    sync::{PyOnceLock, with_critical_section},
    types::{PyDict, PyString, PyType},
};

use crate::{
    model::{ElementList, Reflist},
    namespace::Namespace,
};

#[inline(always)]
pub fn setup(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<Key>()?;

    Ok(())
}

#[pyclass(module = "capellambse._compiled", name = "_Key")]
pub enum Key {
    Child(Py<PyString>),
    Attribute(Py<PyString>),
}

#[pymethods]
impl Key {
    fn __eq__(&self, py: Python<'_>, other: &Key) -> PyResult<bool> {
        match (self, other) {
            (Key::Child(slf), Key::Child(other)) | (Key::Attribute(slf), Key::Attribute(other)) => {
                Ok(slf.bind(py).to_str()? == other.bind(py).to_str()?)
            }
            (Key::Child(_), Key::Attribute(_)) | (Key::Attribute(_), Key::Child(_)) => Ok(false),
        }
    }
}

impl Deref for Key {
    type Target = Py<PyString>;

    fn deref(&self) -> &Self::Target {
        match self {
            Self::Child(k) | Self::Attribute(k) => k,
        }
    }
}

/// A PyAny that has been type-checked to be an AwesomeVersion object.
#[derive(PyWrapper)]
pub struct AwesomeVersion(Py<PyAny>);

impl AwesomeVersion {
    #[inline]
    pub fn cls<'py>(py: Python<'py>) -> PyResult<Bound<'py, PyType>> {
        static CELL: PyOnceLock<Py<PyType>> = PyOnceLock::new();
        CELL.get_or_try_init(py, || {
            Ok(py
                .import(intern!(py, "awesomeversion"))?
                .getattr(intern!(py, "AwesomeVersion"))?
                .cast_into()?
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

impl<'a, 'py> FromPyObject<'a, 'py> for AwesomeVersion {
    type Error = PyErr;

    fn extract(obj: Borrowed<'a, 'py, PyAny>) -> Result<Self, Self::Error> {
        let py = obj.py();
        if let Ok(obj) = obj.cast::<PyString>() {
            Self::new(&obj)
        } else if obj.is_instance(Self::cls(py)?.as_any()).unwrap_or(false) {
            Ok(Self(
                <pyo3::Bound<'_, pyo3::PyAny> as Clone>::clone(&obj).unbind(),
            ))
        } else {
            Err(PyTypeError::new_err(
                "Expected a str with a version number or an AwesomeVersion object",
            ))
        }
    }
}

/// A PyAny that has been type-checked to be a ModelElement.
#[derive(PyWrapper)]
pub struct ModelElement(Py<PyAny>);

impl ModelElement {
    #[inline]
    pub fn cls<'py>(py: Python<'py>) -> PyResult<Bound<'py, PyType>> {
        static CELL: PyOnceLock<Py<PyType>> = PyOnceLock::new();
        CELL.get_or_try_init(py, || {
            Ok(py
                .import(intern!(py, "capellambse.model"))?
                .getattr(intern!(py, "ModelElement"))?
                .cast_into()?
                .unbind())
        })
        .map(|cls| cls.bind(py).clone())
    }

    pub fn new(ns: Bound<Namespace>, clsname: &str, attrs: Bound<PyDict>) -> PyResult<Self> {
        let py = ns.py();
        todo!("cannot make new ModelElement objects yet")
    }

    pub fn id<'py>(&'py self, py: Python<'py>) -> PyResult<Bound<'py, PyString>> {
        Ok(self.0.bind(py).getattr(intern!(py, "uuid"))?.cast_into()?)
    }

    pub fn dict<'py>(&'py self, py: Python<'py>) -> PyResult<Bound<'py, PyDict>> {
        Ok(self
            .0
            .bind(py)
            .getattr(intern!(py, "__dict__"))?
            .cast_into()?)
    }

    pub fn data<'py>(
        &'py self,
        py: Python<'py>,
        key: &Py<Key>,
    ) -> PyResult<Bound<'py, ElementList>> {
        let dict = self.dict(py)?;

        with_critical_section(&dict, || -> PyResult<Bound<'py, ElementList>> {
            Ok(match dict.get_item(&key)? {
                Some(i) => i.cast_into()?,
                None => {
                    let item = ElementList::default().into_pyobject(py)?;
                    dict.set_item(&key, &item)?;
                    item
                }
            })
        })
    }

    pub fn refs<'py>(&'py self, py: Python<'py>) -> PyResult<Bound<'py, Reflist>> {
        let dict = self.dict(py)?;
        let key = {
            static CELL: PyOnceLock<Py<PyAny>> = PyOnceLock::new();
            CELL.get_or_init(py, || {
                let locals = PyDict::new(py);
                py.run(c"k=object()", None, Some(&locals)).unwrap();
                locals.get_item("k").unwrap().unwrap().unbind()
            })
        };

        with_critical_section(&dict, || -> PyResult<Bound<'py, Reflist>> {
            Ok(match dict.get_item(key)? {
                Some(i) => i.cast_into()?,
                None => {
                    let item = Reflist::default().into_pyobject(py)?;
                    dict.set_item(key, &item)?;
                    item
                }
            })
        })
    }
}

impl<'a, 'py> FromPyObject<'a, 'py> for ModelElement {
    type Error = PyErr;

    fn extract(obj: Borrowed<'a, 'py, PyAny>) -> Result<Self, Self::Error> {
        let py = obj.py();
        if obj.is_instance(Self::cls(py)?.as_any())? {
            Ok(Self(
                <pyo3::Bound<'_, pyo3::PyAny> as Clone>::clone(&obj).unbind(),
            ))
        } else {
            Err(PyTypeError::new_err("Expected a ModelElement object"))
        }
    }
}

/// A PyAny that has been type-checked to be a FileHandler instance.
#[derive(PyWrapper)]
pub struct FileHandler(Py<PyAny>);

impl FileHandler {
    pub fn cls<'py>(py: Python<'py>) -> PyResult<Bound<'py, PyType>> {
        static CELL: PyOnceLock<Py<PyType>> = PyOnceLock::new();
        CELL.get_or_try_init(py, || {
            Ok(py
                .import(intern!(py, "capellambse.filehandler"))?
                .getattr(intern!(py, "FileHandler"))?
                .cast_into()?
                .unbind())
        })
        .map(|cls| cls.bind(py).clone())
    }
}

impl<'a, 'py> FromPyObject<'a, 'py> for FileHandler {
    type Error = PyErr;

    fn extract(obj: Borrowed<'a, 'py, PyAny>) -> Result<Self, Self::Error> {
        let py = obj.py();
        if obj.is_instance(Self::cls(py)?.as_any())? {
            Ok(Self(
                <pyo3::Bound<'_, pyo3::PyAny> as Clone>::clone(&obj).unbind(),
            ))
        } else {
            Err(PyTypeError::new_err("Expected a ModelElement object"))
        }
    }
}
