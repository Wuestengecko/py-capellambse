// SPDX-FileCopyrightText: Copyright DB InfraGO AG
// SPDX-License-Identifier: Apache-2.0

use std::{any::type_name, collections::HashMap};

use pyo3::{
    IntoPyObjectExt, PyTraverseError, PyTypeInfo, PyVisit,
    exceptions::*,
    intern,
    prelude::*,
    types::{PyDict, PyMappingProxy, PyString, PyType},
};

use crate::{namespace::Namespace, parse, pytypes::*};

pub type UnresolvedClassName<'py> = (Bound<'py, PyAny>, String);
pub type ClassName = (Py<Namespace>, String);

#[inline(always)]
pub fn setup(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<NativeLoader>()?;
    m.add_class::<ElementList>()?;
    m.add_class::<Namespace>()?;

    m.add_class::<Association>()?;
    m.add_class::<Containment>()?;
    m.add_class::<Backref>()?;

    Ok(())
}

#[pyclass(module = "capellambse._compiled")]
pub struct NativeLoader {
    pub resources: Py<PyDict>,
    pub trees: HashMap<String, Vec<ModelElement>>,
    pub id_index: HashMap<String, ModelElement>,

    corrupt: bool,
}

#[pymethods]
impl NativeLoader {
    #[new]
    #[pyo3(signature=(path, entrypoint = None, *, **kw))]
    fn __new__(
        path: Bound<'_, PyAny>,
        entrypoint: Option<Bound<'_, PyAny>>,
        kw: Option<Bound<'_, PyDict>>,
    ) -> PyResult<Self> {
        let py = path.py();
        let fh_mod = py.import(intern!(py, "capellambse.filehandler"))?;

        let (filehandler, entrypoint): (Bound<'_, PyAny>, Bound<'_, PyAny>) = fh_mod
            .call_method(
                intern!(py, "derive_entrypoint"),
                (path, entrypoint),
                kw.as_ref(),
            )?
            .extract()?;

        let suffix = entrypoint
            .getattr("suffix")
            .and_then(|s| s.extract::<String>());
        if !matches!(suffix, Ok(s) if s == ".aird") {
            return Err(PyValueError::new_err(
                "Invalid entrypoint, specify the ``.aird`` file",
            ));
        }

        let resources = PyDict::new(py);
        resources.set_item("\x00", filehandler)?;

        let mut model = Self {
            resources: resources.into_pyobject(py)?.unbind(),
            trees: HashMap::new(),
            id_index: HashMap::new(),
            corrupt: false,
        };

        parse::parse_from_resources(&mut model, entrypoint)?;

        Ok(model)
    }

    pub fn referenced_viewpoints(&self, py: Python<'_>) -> PyResult<Py<PyDict>> {
        Ok(PyDict::new(py).unbind())
    }

    pub fn by_uuid(&self, py: Python<'_>, uuid: &str) -> PyResult<ModelElement> {
        self.id_index
            .get(uuid)
            .map(|e| e.clone_ref(py))
            .ok_or_else(|| PyKeyError::new_err(uuid.to_owned()).into())
    }

    pub fn mark_corrupt(&mut self) {
        self.corrupt = true;
    }
}

#[pyclass(module = "capellambse._compiled")]
struct Containment {
    name: Option<Py<Key>>,
    clsname: ClassName,
    pyname: Option<Py<PyString>>,
    mapkey: Option<Py<PyString>>,
    mapvalue: Option<Py<PyString>>,
    alternate: Option<Py<PyType>>,
    single_attr: Option<Py<PyString>>,
    fixed_length: usize,
    type_hint_map: Option<HashMap<String, ClassName>>,

    #[pyo3(get)]
    __objclass__: Option<Py<PyType>>,
    #[pyo3(get)]
    __name__: Py<PyString>,
    #[pyo3(get)]
    __doc__: Py<PyString>,
}

#[pymethods]
impl Containment {
    #[new]
    #[pyo3(signature = (name, clsname, /, *, mapkey = None, mapvalue = None, alternate = None, single_attr = None, fixed_length = 0, type_hint_map = None))]
    fn __new__<'py>(
        py: Python<'py>,
        name: Option<Py<PyString>>,
        clsname: UnresolvedClassName,
        mapkey: Option<Py<PyString>>,
        mapvalue: Option<Py<PyString>>,
        alternate: Option<Bound<'_, PyType>>,
        single_attr: Option<Py<PyString>>,
        fixed_length: usize,
        type_hint_map: Option<HashMap<String, UnresolvedClassName>>,
    ) -> PyResult<Self> {
        let type_hint_map = if let Some(type_hint_map) = type_hint_map {
            Some(
                type_hint_map
                    .into_iter()
                    .map(|(k, cn)| Ok((k, resolve_class_name(cn)?)))
                    .collect::<PyResult<_>>()?,
            )
        } else {
            None
        };

        let alternate = if let Some(alternate) = alternate {
            let model_element = ModelElement::cls(py)?;
            if !alternate.is_subclass(&model_element).unwrap_or(false) {
                Err(PyTypeError::new_err(
                    "'alternate' must be a subclass of ModelElement",
                ))?
            }
            Some(alternate.unbind())
        } else {
            None
        };

        let name = if let Some(name) = name {
            Some(Key::Child(name).into_pyobject(py)?.unbind())
        } else {
            None
        };

        Ok(Self {
            name,
            clsname: resolve_class_name(clsname)?,
            pyname: None,
            mapkey,
            mapvalue,
            alternate,
            single_attr,
            fixed_length: fixed_length,
            type_hint_map,

            __objclass__: None,
            __name__: intern!(py, "<unknown>").clone().unbind(),
            __doc__: intern!(py, "A Relation that was not properly configured. Ensure that '__set_name__' gets called after construction.").clone().unbind(),
        })
    }

    #[pyo3(signature = (owner, name, /))]
    fn __set_name__(
        mut slf: PyRefMut<'_, Self>,
        py: Python<'_>,
        owner: Bound<'_, PyType>,
        name: Bound<'_, PyString>,
    ) -> PyResult<()> {
        let name_str = name.to_str()?;
        if slf.name.is_none()
            && let Some(overridden) = find_overridden_relation::<Self>(&owner, name_str)?
        {
            let overridden = overridden.borrow();
            slf.__doc__ = overridden.__doc__.clone_ref(py);
            slf.name = overridden.name.as_ref().map(|n| n.clone_ref(py));
            if slf.fixed_length == 0 {
                slf.fixed_length = overridden.fixed_length;
            }
            if slf.mapkey.is_none() {
                slf.mapkey = overridden.mapkey.as_ref().map(|k| k.clone_ref(py));
                if slf.mapvalue.is_none() {
                    slf.mapvalue = overridden.mapvalue.as_ref().map(|v| v.clone_ref(py));
                }
            }
            if slf.single_attr.is_none() {
                slf.single_attr = overridden.single_attr.as_ref().map(|s| s.clone_ref(py));
            }
        } else {
            slf.__doc__ = gendocstring(&owner, name_str).unbind();
        }

        if slf.name.is_none() {
            let owner_name = match owner.name() {
                Ok(name) => name.to_string_lossy().to_string(),
                Err(_) => "<unknown owner>".into(),
            };
            Err(PyTypeError::new_err(format!(
                "{} '{}.{}' requires a 'name', but it wasn't specified and no super class has it defined",
                type_name::<Self>(),
                owner_name,
                name_str,
            )))?
        }

        slf.__name__ = name.unbind();
        slf.__objclass__ = Some(owner.unbind());

        Ok(())
    }

    fn __get__(
        slf: PyRef<'_, Self>,
        py: Python<'_>,
        obj: Option<ModelElement>,
        _objtype: &Bound<'_, PyType>,
    ) -> PyResult<Py<PyAny>> {
        let Some(obj) = obj else {
            return Ok(slf.into_py_any(py)?);
        };

        Ok(slf.get(py, &obj)?.into_py_any(py)?)
    }

    fn __set__(&self, py: Python<'_>, obj: ModelElement, value: Vec<ModelElement>) -> PyResult<()> {
        let mut data = self.get(py, &obj)?.borrow_mut();
        for (i, obj) in value.into_iter().enumerate() {
            data.insert(i as isize, obj)?;
        }
        Ok(())
    }

    fn __delete__(&self, py: Python<'_>, obj: ModelElement) -> PyResult<()> {
        let mut data = self.get(py, &obj)?.try_borrow_mut()?;
        data.clear()?;
        Ok(())
    }

    fn __repr__(&self, py: Python<'_>) -> String {
        let (Some(owner), Some(pyname), Some(name)) = (
            self.__objclass__.as_ref(),
            self.pyname.as_ref(),
            self.name.as_ref(),
        ) else {
            return "<unknown Containment - call __set_name__>".into();
        };
        let owner = py_name(owner.bind(py));

        let typename = type_name::<Self>();
        let ns = self.clsname.0.borrow(py);
        let nsalias = ns.alias.as_str();
        let clsname = self.clsname.1.as_str();
        format!("<{typename} '{owner}.{pyname}' of {nsalias}:{clsname} in {name}>")
    }
}

impl Containment {
    fn get<'py>(
        &'py self,
        py: Python<'py>,
        obj: &'py ModelElement,
    ) -> PyResult<Bound<'py, ElementList>> {
        let Some(ref name) = self.name else {
            Err(PyRuntimeError::new_err(
                "Relationship descriptor was not initialized properly; make sure that __set_name__ gets called",
            ))?
        };
        obj.data(py, name)
    }
}

#[pyclass(module = "capellambse._compiled")]
struct Association {
    name: Option<Py<Key>>,
    clsname: ClassName,
    pyname: Option<Py<PyString>>,
    mapkey: Option<Py<PyString>>,
    mapvalue: Option<Py<PyString>>,
    fixed_length: usize,

    #[pyo3(get)]
    __objclass__: Option<Py<PyType>>,
    #[pyo3(get)]
    __name__: Py<PyString>,
    #[pyo3(get)]
    __doc__: Py<PyString>,
}

#[pymethods]
impl Association {
    #[new]
    #[pyo3(signature = (clsname, name, /, *, mapkey = None, mapvalue = None, fixed_length = 0))]
    fn __new__(
        py: Python<'_>,
        clsname: (Bound<'_, PyAny>, String),
        name: Option<Py<PyString>>,
        mapkey: Option<Py<PyString>>,
        mapvalue: Option<Py<PyString>>,
        fixed_length: usize,
    ) -> PyResult<Self> {
        let name = if let Some(name) = name {
            Some(Key::Attribute(name).into_pyobject(py)?.unbind())
        } else {
            None
        };
        Ok(Self {
            name,
            clsname: resolve_class_name(clsname)?,
            pyname: None,
            mapkey: mapkey,
            mapvalue: mapvalue,
            fixed_length: fixed_length,

            __objclass__: None,
            __name__: intern!(py, "<unknown>").clone().unbind(),
            __doc__: intern!(py, "A Relation that was not properly configured. Ensure that '__set_name__' gets called after construction.").clone().unbind(),
        })
    }

    #[pyo3(signature = (owner, name, /))]
    fn __set_name__(
        mut slf: PyRefMut<'_, Self>,
        py: Python<'_>,
        owner: Bound<'_, PyType>,
        name: Bound<'_, PyString>,
    ) -> PyResult<()> {
        let name_str = name.to_str()?;
        if slf.name.is_none()
            && let Some(overridden) = find_overridden_relation::<Self>(&owner, name_str)?
        {
            let overridden = overridden.borrow();
            slf.__doc__ = overridden.__doc__.clone_ref(py);
            slf.name = overridden.name.as_ref().map(|n| n.clone_ref(py));
            if slf.fixed_length == 0 {
                slf.fixed_length = overridden.fixed_length;
            }
            if slf.mapkey.is_none() {
                slf.mapkey = overridden.mapkey.as_ref().map(|k| k.clone_ref(py));
                if slf.mapvalue.is_none() {
                    slf.mapvalue = overridden.mapvalue.as_ref().map(|v| v.clone_ref(py));
                }
            }
        } else {
            slf.__doc__ = gendocstring(&owner, name_str).unbind();
        }

        if slf.name.is_none() {
            let owner_name = match owner.name() {
                Ok(name) => name.to_string_lossy().to_string(),
                Err(_) => "<unknown owner>".into(),
            };
            Err(PyTypeError::new_err(format!(
                "{} '{}.{}' requires a 'name', but it wasn't specified and no super class has it defined",
                type_name::<Self>(),
                owner_name,
                name_str,
            )))?
        }

        slf.__name__ = name.unbind();
        slf.__objclass__ = Some(owner.unbind());
        Ok(())
    }

    fn __get__(
        slf: PyRef<'_, Self>,
        py: Python<'_>,
        obj: Option<ModelElement>,
        _objtype: &Bound<'_, PyType>,
    ) -> PyResult<Py<PyAny>> {
        let Some(obj) = obj else {
            return Ok(slf.into_py_any(py)?);
        };

        let Some(ref name) = slf.name else {
            Err(PyRuntimeError::new_err(
                "Relationship descriptor was not initialized properly; make sure that __set_name__ gets called",
            ))?
        };
        Ok(obj.data(py, name)?.into_py_any(py)?)
    }

    fn __set__(&self, _obj: ModelElement, _value: &Bound<'_, PyAny>) -> PyResult<()> {
        Err(PyNotImplementedError::new_err("not yet implemented"))
    }

    fn __delete__(&self, _obj: ModelElement) -> PyResult<()> {
        Err(PyNotImplementedError::new_err("not yet implemented"))
    }

    fn __repr__(&self, py: Python<'_>) -> String {
        let (Some(owner), Some(pyname), Some(name)) = (
            self.__objclass__.as_ref(),
            self.pyname.as_ref(),
            self.name.as_ref(),
        ) else {
            return "<unknown Backref - call __set_name__>".into();
        };
        let owner = py_name(owner.bind(py));

        let typename = type_name::<Self>();
        let ns = self.clsname.0.borrow(py);
        let nsalias = ns.alias.as_str();
        let clsname = self.clsname.1.as_str();
        format!("<{typename} '{owner}.{pyname}' to {nsalias}:{clsname} on {name}>")
    }
}

#[pyclass(module = "capellambse._compiled")]
struct Backref {
    clsname: ClassName,
    attrs: Vec<Py<PyString>>,
    pyname: Option<Py<PyString>>,
    mapkey: Option<Py<PyString>>,
    mapvalue: Option<Py<PyString>>,

    #[pyo3(get)]
    __objclass__: Option<Py<PyType>>,
    #[pyo3(get)]
    __name__: Py<PyString>,
    #[pyo3(get)]
    __doc__: Py<PyString>,
}

#[pymethods]
impl Backref {
    #[new]
    #[pyo3(signature = (clsname, /, *attrs, mapkey = None, mapvalue = None))]
    fn __new__(
        py: Python<'_>,
        clsname: (Bound<'_, PyAny>, String),
        attrs: Vec<Py<PyString>>,
        mapkey: Option<Py<PyString>>,
        mapvalue: Option<Py<PyString>>,
    ) -> PyResult<Self> {
        Ok(Self {
            clsname: resolve_class_name(clsname)?,
            attrs,
            pyname: None,
            mapkey,
            mapvalue,

            __objclass__:  None,
            __name__:  intern!(py, "<unknown>").clone().unbind(),
            __doc__: intern!(py, "A Relation that was not properly configured. Ensure that '__set_name__' gets called after construction.").clone().unbind(),
        })
    }

    #[pyo3(signature = (owner, name, /))]
    fn __set_name__(
        mut slf: PyRefMut<'_, Self>,
        py: Python<'_>,
        owner: Bound<'_, PyType>,
        name: Bound<'_, PyString>,
    ) -> PyResult<()> {
        let name_str = name.to_str()?;
        if slf.attrs.len() == 0
            && let Some(overridden) = find_overridden_relation::<Self>(&owner, name_str)?
        {
            let overridden = overridden.borrow();
            slf.__doc__ = overridden.__doc__.clone_ref(py);
            slf.attrs = overridden.attrs.iter().map(|n| n.clone_ref(py)).collect();
            if slf.mapkey.is_none() {
                slf.mapkey = overridden.mapkey.as_ref().map(|k| k.clone_ref(py));
                if slf.mapvalue.is_none() {
                    slf.mapvalue = overridden.mapvalue.as_ref().map(|v| v.clone_ref(py));
                }
            }
        } else {
            slf.__doc__ = gendocstring(&owner, name_str).unbind();
        }

        if slf.attrs.len() == 0 {
            let owner_name = match owner.name() {
                Ok(name) => name.to_string_lossy().to_string(),
                Err(_) => "<unknown owner>".into(),
            };
            Err(PyTypeError::new_err(format!(
                "{} '{}.{}' requires a 'name', but it wasn't specified and no super class has it defined",
                type_name::<Self>(),
                owner_name,
                name_str,
            )))?
        }

        slf.__name__ = name.unbind();
        slf.__objclass__ = Some(owner.unbind());
        Ok(())
    }

    fn __get__(
        slf: PyRef<'_, Self>,
        py: Python<'_>,
        obj: Option<ModelElement>,
        _objtype: &Bound<'_, PyType>,
    ) -> PyResult<Py<PyAny>> {
        let Some(obj) = obj else {
            return Ok(slf.into_py_any(py)?);
        };

        if slf.attrs.len() == 0 {
            Err(PyRuntimeError::new_err(
                "Relationship descriptor was not initialized properly; make sure that __set_name__ gets called",
            ))?
        };

        let refs = obj.refs(py)?.into_py_any(py)?;
        Err(PyNotImplementedError::new_err("not yet implemented")) // TODO
    }

    fn __set__(
        &self,
        py: Python<'_>,
        _obj: ModelElement,
        _value: &Bound<'_, PyAny>,
    ) -> PyResult<()> {
        Err(PyTypeError::new_err(format!(
            "Cannot set {} {}.{}, modify {:?} of {} instead",
            type_name::<Self>(),
            self.__objclass__
                .as_ref()
                .and_then(|n| n.bind(py).name().and_then(|n| n.extract::<String>()).ok())
                .unwrap_or_else(|| "<unknown>".into()),
            self.pyname
                .as_ref()
                .and_then(|n| n.extract::<String>(py).ok())
                .unwrap_or_else(|| "<unknown>".into()),
            self.attrs,
            self.clsname.1,
        )))
    }

    fn __delete__(&self, py: Python<'_>, _obj: ModelElement) -> PyResult<()> {
        Err(PyTypeError::new_err(format!(
            "Cannot delete {} {}.{}, delete self from {:?} of {} instead",
            type_name::<Self>(),
            self.__objclass__
                .as_ref()
                .and_then(|n| n.bind(py).name().and_then(|n| n.extract::<String>()).ok())
                .unwrap_or_else(|| "<unknown>".into()),
            self.pyname
                .as_ref()
                .and_then(|n| n.extract::<String>(py).ok())
                .unwrap_or_else(|| "<unknown>".into()),
            self.attrs,
            self.clsname.1,
        )))
    }

    fn __repr__(&self, py: Python<'_>) -> String {
        let (Some(owner), Some(pyname)) = (self.__objclass__.as_ref(), self.pyname.as_ref()) else {
            return "<unknown Backref - call __set_name__>".into();
        };
        let owner = py_name(owner.bind(py));

        let typename = type_name::<Self>();
        let ns = self.clsname.0.borrow(py);
        let nsalias = ns.alias.as_str();
        let clsname = self.clsname.1.as_str();
        let attrs = &self.attrs;
        format!("<{typename} '{owner}.{pyname}' to {nsalias}:{clsname} through {attrs:?}>")
    }
}

#[derive(Default)]
#[pyclass(module = "capellambse._compiled", sequence)]
pub struct ElementList {
    inner: Vec<ModelElement>,
}

#[pymethods]
impl ElementList {
    #[classattr]
    const __hash__: Option<Py<PyAny>> = None;

    fn __repr__(&self, py: Python<'_>) -> PyResult<String> {
        let mut buf = "<ElementList [".to_owned();
        for (i, elem) in self.inner.iter().enumerate() {
            if i > 0 {
                buf.extend(", ".chars());
            }
            buf.extend(elem.bind(py).repr()?.to_str()?.chars());
        }
        buf.extend("]>".chars());
        Ok(buf)
    }

    fn __eq__(&self, py: Python<'_>, other: Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
        let Ok(other) = other.try_iter() else {
            return Ok(py.NotImplemented());
        };
        for (left, right) in self.inner.iter().zip(other) {
            let right = right?;
            if !left.is(&right) {
                return Ok(false.into_py_any(py)?);
            }
        }
        true.into_py_any(py)
    }

    fn __bool__(&self) -> bool {
        self.inner.len() > 0
    }

    fn __iter__(slf: Py<Self>) -> ElementListIterator {
        ElementListIterator {
            parent: slf,
            idx: 0,
        }
    }

    fn __len__(&self) -> usize {
        self.inner.len()
    }

    fn __contains__(&self, needle: Py<PyAny>) -> bool {
        self.inner.iter().any(|e| e.is(&needle))
    }

    fn __getitem__(&self, py: Python<'_>, idx: Bound<PyAny>) -> PyResult<ModelElement> {
        if let Ok(idx) = idx.extract::<isize>() {
            let idx = if idx >= 0 {
                idx
            } else {
                idx + (self.inner.len() as isize)
            };
            if idx > 0
                && let Some(elem) = self.inner.get(idx as usize)
            {
                return Ok(elem.clone_ref(py));
            }
            Err(PyIndexError::new_err("ElementList index out of range"))?
        }

        Err(PyNotImplementedError::new_err(
            "ElementList slicing is not implemented yet",
        ))?
    }

    fn __setitem__(&mut self, _idx: Bound<PyAny>, _value: Bound<PyAny>) -> PyResult<()> {
        Err(PyNotImplementedError::new_err("not yet implemented")) // TODO
    }

    fn __delitem__(&mut self, _idx: Bound<PyAny>) -> PyResult<()> {
        Err(PyNotImplementedError::new_err("not yet implemented")) // TODO
    }

    fn __concat__(&self, py: Python<'_>, other: Bound<PyAny>) -> PyResult<Py<PyAny>> {
        let mut result = Vec::with_capacity(self.inner.len() + other.len().unwrap_or(0));
        result.extend(self.inner.iter().map(|e| e.as_any().clone_ref(py)));
        for i in other.try_iter()? {
            result.push(i?.into_py_any(py)?);
        }
        result.into_py_any(py)
    }

    fn __repeat__(&self, py: Python<'_>, n: usize) -> Vec<ModelElement> {
        let mut result = Vec::with_capacity(self.inner.len() * n);
        for _ in 0..n {
            result.extend(self.inner.iter().map(|e| e.clone_ref(py)));
        }
        result
    }

    fn __traverse__(&self, visit: PyVisit<'_>) -> Result<(), PyTraverseError> {
        self.inner.iter().try_for_each(|e| visit.call(&**e))
    }

    fn __clear__(&mut self, py: Python<'_>) {
        while let Some(obj) = self.inner.pop() {
            obj.drop_ref(py);
        }
    }

    fn __iadd__(&mut self, value: Bound<'_, PyAny>) -> PyResult<()> {
        self.extend(value)
    }

    #[pyo3(signature = (value, /))]
    fn append(&mut self, value: ModelElement) -> PyResult<()> {
        Err(PyNotImplementedError::new_err("not yet implemented")) // TODO
    }

    fn clear(&mut self) -> PyResult<()> {
        Err(PyNotImplementedError::new_err("not yet implemented")) // TODO
    }

    #[pyo3(signature = (value, /))]
    fn count(&self, py: Python<'_>, value: ModelElement) -> usize {
        let value = value.bind(py);
        self.inner.iter().filter(|&i| value.is(&**i)).count()
    }

    #[pyo3(signature = (iterable, /))]
    fn extend(&mut self, iterable: Bound<'_, PyAny>) -> PyResult<()> {
        let it = iterable.try_iter()?;
        for elem in it {
            let elem = elem?.extract()?;
            self.append(elem)?;
        }
        Ok(())
    }

    #[pyo3(signature = (value, start = 0, stop = usize::MAX))]
    fn index(
        &self,
        py: Python<'_>,
        value: ModelElement,
        start: usize,
        stop: usize,
    ) -> PyResult<usize> {
        let value = value.bind(py);
        self.inner
            .iter()
            .enumerate()
            .take(stop)
            .skip(start)
            .find_map(|(i, elem)| value.is(&**elem).then_some(i))
            .ok_or_else(|| PyValueError::new_err("Element not found in list"))
    }

    #[pyo3(signature = (before, value, /))]
    fn insert(&mut self, before: isize, value: ModelElement) -> PyResult<()> {
        Err(PyNotImplementedError::new_err("not yet implemented")) // TODO
    }

    #[pyo3(signature = (idx = -1, /))]
    fn pop(&mut self, idx: isize) -> PyResult<ModelElement> {
        Err(PyNotImplementedError::new_err("not yet implemented")) // TODO
    }

    #[pyo3(signature = (value, /))]
    fn remove(&mut self, value: ModelElement) -> PyResult<()> {
        Err(PyNotImplementedError::new_err("not yet implemented")) // TODO
    }

    fn reverse(&mut self) {
        self.inner.reverse();
    }
}

#[pyclass(module = "capellambse._compiled")]
struct ElementListIterator {
    parent: Py<ElementList>,
    idx: usize,
}

#[pymethods]
impl ElementListIterator {
    fn __iter__(slf: PyRef<'_, Self>) -> PyRef<'_, Self> {
        slf
    }

    fn __next__(&mut self, py: Python<'_>) -> Option<ModelElement> {
        let parent = self.parent.borrow(py);
        let item = parent.inner.get(self.idx)?;
        self.idx += 1;
        Some(item.clone_ref(py))
    }
}

#[derive(Default)]
#[pyclass(module = "capellambse._compiled")]
pub struct Reflist {
    inner: Vec<(ModelElement, Py<Key>)>,
}

#[pyclass(module = "capellambse._compiled")]
struct PyReflist {
    parent: Py<Reflist>,
}

fn resolve_class_name<'py>(clsname: UnresolvedClassName<'py>) -> PyResult<ClassName> {
    let py = clsname.0.py();
    let (ns, clsname) = getfunc(intern!(py, "resolve_class_name"))
        .call1((clsname,))?
        .extract::<(Bound<'_, PyAny>, String)>()
        .expect("Unexpected return type from 'resolve_class_name'");
    let ns = ns.cast_into()?.unbind();
    Ok((ns, clsname))
}

fn getfunc<'py>(name: &Bound<'py, PyString>) -> Bound<'py, PyAny> {
    let py = name.py();
    py.import(intern!(py, "capellambse.model"))
        .expect("cannot import capellambse.model")
        .getattr(name)
        .expect("cannot find required class/function on capellambse.model")
}

pub fn getclass<'py>(name: &Bound<'py, PyString>) -> Bound<'py, PyType> {
    getfunc(name)
        .cast_into()
        .expect("expected a class, got non-type object")
}

fn py_name(obj: &Bound<'_, PyType>) -> String {
    obj.name()
        .and_then(|v| v.extract::<String>())
        .unwrap_or_else(|_| "<unknown>".into())
}

fn find_overridden_relation<'py, R: PyTypeInfo>(
    owner: &Bound<'py, PyAny>,
    name: &str,
) -> PyResult<Option<Bound<'py, R>>> {
    let py = owner.py();
    let mut mro = owner.getattr(intern!(py, "__mro__"))?.try_iter()?;
    if let Some(i) = mro.next() {
        i?;
    }
    let name = PyString::new(py, name);

    Ok(loop {
        let Some(cls) = mro.next() else { break None };
        let cls = cls?;
        let dict = cls
            .getattr(intern!(py, "__dict__"))?
            .cast_into::<PyMappingProxy>()?;
        let Ok(mut rel) = dict.get_item(&name) else {
            continue;
        };
        if rel.is_instance(&getclass(intern!(py, "Single")))? {
            rel = rel.getattr(intern!(py, "wrapped"))?;
        }
        let Ok(rel) = rel.getattr(intern!(py, "__impl")) else {
            break None;
        };
        let Ok(rel) = rel.cast_into() else {
            break None;
        };
        break Some(rel);
    })
}

fn gendocstring<'py>(owner: &'py Bound<'py, PyAny>, name: &str) -> Bound<'py, PyString> {
    let py = owner.py();
    let owner = owner.getattr(intern!(py, "__name__"));
    let owner = match owner {
        Ok(ref owner) => owner.extract::<&str>().unwrap_or("type"),
        Err(_) => "type",
    };
    let s = format!("The {} of this {}.", name.replace('_', " "), owner);
    PyString::new(py, &s)
}
