// SPDX-FileCopyrightText: Copyright DB InfraGO AG
// SPDX-License-Identifier: Apache-2.0

use std::{cmp::Ordering, collections::HashMap};

use pyo3::IntoPyObjectExt;
use pyo3::exceptions::{PyTypeError, PyValueError};
use pyo3::intern;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyString, PyType};

use crate::pytypes::AwesomeVersion;

#[inline(always)]
pub fn setup(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<Namespace>()?;

    Ok(())
}

#[pyclass(module = "capellambse._compiled")]
pub struct Namespace {
    #[pyo3(get)]
    pub uri: String,
    #[pyo3(get)]
    pub alias: String,
    #[pyo3(get)]
    pub viewpoint: Option<String>,
    pub maxver: Option<Vec<String>>,

    /// Number of significant parts in the version number for namespaces.
    ///
    /// When qualifying a versioned namespace based on the model's activated
    /// viewpoint, only use this many components for the namespace URL.
    /// Components after that are set to zero.
    ///
    /// Example: A viewpoint version of "1.2.3" with a version precision of
    /// 2 will result in the namespace version "1.2.0".
    #[pyo3(get)]
    pub version_precision: usize,

    pub classes: HashMap<String, Vec<(Py<PyType>, AwesomeVersion, Option<AwesomeVersion>)>>,
}

#[pymethods]
impl Namespace {
    #[new]
    #[pyo3(signature = (uri, alias, viewpoint = None, maxver = None, *, version_precision = 1))]
    pub fn __new__(
        uri: String,
        alias: String,
        viewpoint: Option<String>,
        maxver: Option<String>,
        version_precision: usize,
    ) -> PyResult<Self> {
        if version_precision < 1 {
            Err(PyValueError::new_err(
                "Version precision must be greater than zero",
            ))?
        }

        let is_versioned = uri.contains("{VERSION}");
        if is_versioned && maxver.is_none() {
            Err(PyTypeError::new_err(
                "Versioned namespaces must declare their supported 'maxver'",
            ))?
        }
        if !is_versioned && maxver.is_some() {
            Err(PyTypeError::new_err(
                "Unversioned namespaces cannot declare a supported 'maxver'",
            ))?
        }

        let maxver = maxver.map(|v| v.split('.').map(|i| i.to_owned()).collect());

        Ok(Self {
            uri,
            alias,
            viewpoint,
            maxver,
            version_precision,
            classes: HashMap::new(),
        })
    }

    #[getter]
    pub fn get_maxver(&self) -> Option<String> {
        self.maxver.as_ref().map(|v| v.join("."))
    }

    #[cfg(debug_assertions)]
    #[getter(_classes)]
    pub fn get_classes<'py>(&'py self, py: Python<'py>) -> PyResult<Bound<'py, PyDict>> {
        (&self.classes).into_pyobject(py)
    }

    /// Match a (potentially versioned) URI against this namespace.
    ///
    /// The return type depends on whether this namespace is versioned.
    ///
    /// Unversioned Namespaces return a simple boolean flag indicating
    /// whether the URI exactly matches this Namespace.
    ///
    /// Versioned Namespaces return one of:
    ///
    /// - ``False``, if the URI did not match
    /// - ``None``, if the URI did match, but the version field was
    ///   empty or the literal ``{VERSION}`` placeholder
    /// - Otherwise, an :class:~`awesomeversion.AwesomeVersion` object
    ///   with the version number contained in the URL
    ///
    /// Values other than True and False can then be passed on to
    /// :meth:`get_class`, to obtain a class object appropriate for the
    /// namespace and version described by the URI.
    #[pyo3(signature = (uri, /))]
    pub fn match_uri(&self, py: Python<'_>, uri: &str) -> PyResult<Py<PyAny>> {
        if let Some((prefix, suffix)) = self.uri.split_once("{VERSION}") {
            if uri.len() >= prefix.len() + suffix.len()
                && uri.starts_with(prefix)
                && uri.ends_with(suffix)
            {
                let v = &uri[prefix.len()..(uri.len() - suffix.len())];
                if v.contains("/") {
                    return Ok(false.into_py_any(py)?);
                }
                if v == "" || v == "{VERSION}" {
                    return Ok(py.None());
                }
                let v = self.trim_version(v);
                return Ok(AwesomeVersion::new(&PyString::new(py, &v))?.into());
            }
            Ok(false.into_py_any(py)?)
        } else {
            Ok((uri == self.uri).into_py_any(py)?)
        }
    }

    #[pyo3(signature = (clsname, /, version = None))]
    pub fn get_class(
        slf: PyRef<Self>,
        py: Python<'_>,
        clsname: &str,
        version: Option<AwesomeVersion>,
    ) -> PyResult<Py<PyType>> {
        let is_versioned = slf.uri.contains("{VERSION}");
        if is_versioned && version.is_none() {
            Err(PyTypeError::new_err(format!(
                "Versioned namespace, but no version requested: {}",
                slf.uri
            )))?
        }

        let candidates = slf
            .classes
            .get(clsname)
            .map(|classes| {
                let mut c = classes
                    .iter()
                    .filter_map(|(cls, minver, maxver)| {
                        let minver = minver.bind(py);
                        match version {
                            None => Some((minver, cls.bind(py))),
                            Some(ref version)
                                if minver.le(version).unwrap_or(false)
                                    && maxver
                                        .as_ref()
                                        .map(|maxver| maxver.ge(py, version).unwrap_or(false))
                                        .unwrap_or(true) =>
                            {
                                Some((minver, cls.bind(py)))
                            }
                            _ => None,
                        }
                    })
                    .collect::<Vec<_>>();
                c.sort_by(|left, right| right.0.compare(left.0).unwrap_or(Ordering::Equal));
                c
            })
            .unwrap_or_default();

        let Some(cls) = candidates.get(0) else {
            Err(PyErr::from_type(
                getclass(intern!(py, "MissingClassError")),
                (
                    slf.into_pyobject(py).unwrap().unbind(),
                    version,
                    clsname.to_owned(),
                ),
            ))?
        };
        Ok(cls.1.clone().unbind())
    }

    #[pyo3(signature = (cls, /, minver, maxver))]
    pub fn register(
        slf: Bound<'_, Self>,
        py: Python<'_>,
        cls: Bound<'_, PyType>,
        minver: Option<Bound<PyString>>,
        maxver: Option<Bound<PyString>>,
    ) -> PyResult<()> {
        let clsname = py_name(&cls);
        let cls_ns = cls.getattr("__capella_namespace__")?.cast_into::<Self>()?;
        if !cls_ns.is(&slf) {
            let slf = slf.borrow();
            let cls_ns = cls_ns.borrow();
            Err(PyValueError::new_err(format!(
                "Cannot register class {} in Namespace {} because it belongs to {}",
                clsname, slf.uri, cls_ns.uri,
            )))?
        }

        let mut slf = slf.borrow_mut();
        let classes = slf.classes.entry(clsname).or_insert_with(Vec::new);
        let minver = match minver {
            None => AwesomeVersion::new(intern!(py, "0"))?,
            Some(minver) => AwesomeVersion::new(&minver)?,
        };
        let maxver = match maxver {
            None => None,
            Some(maxver) => Some(AwesomeVersion::new(&maxver)?),
        };
        classes.push((cls.unbind(), minver, maxver));

        Ok(())
    }

    #[pyo3(signature = (version, /))]
    pub fn trim_version(&self, version: &str) -> String {
        assert!(self.version_precision > 0);
        let mut parts: Vec<_> = version.split('.').collect();
        parts[self.version_precision..]
            .iter_mut()
            .for_each(|i| *i = "0");
        parts.join(".")
    }

    pub fn __contains__(&self, clsname: &str) -> bool {
        self.classes.contains_key(clsname)
    }
}

fn getfunc<'py>(name: &Bound<'py, PyString>) -> Bound<'py, PyAny> {
    let py = name.py();
    py.import(intern!(py, "capellambse.model"))
        .expect("cannot import capellambse.model")
        .getattr(name)
        .expect("cannot find required class/function on capellambse.model")
}

fn getclass<'py>(name: &Bound<'py, PyString>) -> Bound<'py, PyType> {
    getfunc(name)
        .cast_into()
        .expect("expected a class, got non-type object")
}

fn py_name(obj: &Bound<'_, PyType>) -> String {
    obj.name()
        .and_then(|v| v.extract::<String>())
        .unwrap_or_else(|_| "<unknown>".into())
}
