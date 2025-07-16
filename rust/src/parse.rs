// SPDX-FileCopyrightText: Copyright DB InfraGO AG
// SPDX-License-Identifier: Apache-2.0

use std::{collections::HashMap, convert::Infallible, io::BufReader, sync::Arc};

use pyo3::{
    exceptions::{
        PyIndexError, PyNotImplementedError, PyOSError, PyRuntimeError, PyUnicodeDecodeError,
        PyValueError,
    },
    intern,
    prelude::*,
    types::{PyBytes, PyDict, PyString},
};
use quick_xml::{Reader, events::BytesStart};

use crate::{
    model::{NativeLoader, getclass},
    namespace::Namespace,
    pytypes::ModelElement,
};

const NAMESPACE_XSI: &[u8] = b"http://www.w3.org/2001/XMLSchema-instance";

pub fn parse_from_resources(
    model: &mut NativeLoader,
    entrypoint: Bound<'_, PyAny>,
) -> PyResult<()> {
    let py = entrypoint.py();

    let pyreader = PyReader::open(model.resources.bind(py), "\x00", &entrypoint)?;
    let mut reader = Reader::from_reader(BufReader::new(pyreader));

    let mut string_cache = StringCache::default();
    let mut stack = Vec::new();
    let mut buf = Vec::new();
    loop {
        use quick_xml::events::Event as E;
        match reader.read_event_into(&mut buf) {
            Err(e) => Err(PyOSError::new_err(format!(
                "Could not parse XML: {}",
                e.to_string()
            )))?,
            Ok(E::Eof) => break,
            Ok(E::Start(ev)) => {
                let elm = parse_element(py, model, &mut stack, ev, &mut string_cache)?;
                stack.push(elm);
            }
            Ok(E::Empty(ev)) => {
                let elm = parse_element(py, model, &stack, ev, &mut string_cache)?;
                finish_element(py, model, &stack, elm)?;
            }
            Ok(E::End(_)) => {
                let elm = stack.pop().unwrap();
                finish_element(py, model, &stack, elm)?;
            }
            Ok(E::Text(ev))
                if ev
                    .decode()
                    .map(|t| t.chars().all(|c| c.is_whitespace()))
                    .unwrap_or(false) => {}
            Ok(E::Text(ev)) => match stack
                .last_mut()
                .ok_or_else(|| PyValueError::new_err("orphaned text at document root?"))?
            {
                (_, AnyElement::ModelElement(elm)) => Err(PyValueError::new_err(format!(
                    "unhandled text directly within element {}",
                    elm.id(py)
                        .map(|id| id.to_string_lossy().to_string())
                        .unwrap_or_else(|_| "<unknown id>".into())
                )))?,
                (_, AnyElement::XMLElement(elm)) => {
                    elm.borrow_mut(py).text = Some(
                        ev.xml_content()
                            .map_err(|e| PyUnicodeDecodeError::new_err(e.to_string()))?
                            .to_string(),
                    )
                }
            },
            Ok(E::CData(ev)) => todo!("encountered CData in the XML"),
            Ok(E::GeneralRef(ent)) => todo!("encountered GeneralRef in the XML"),
            Ok(E::Comment(_)) => (),
            Ok(E::DocType(_)) => (),
            Ok(E::Decl(_)) => (),
            Ok(E::PI(_)) => (),
        }
        buf.clear();
    }

    Ok(())
}

fn parse_element(
    py: Python<'_>,
    model: &mut NativeLoader,
    parents: &[(HashMap<String, String>, AnyElement)],
    event: BytesStart<'_>,
    string_cache: &mut StringCache,
) -> PyResult<(HashMap<String, String>, AnyElement)> {
    let mut namespaces = HashMap::new();
    let mut attrs: HashMap<String, Py<PyString>> = HashMap::new();
    let mut xtype = None;
    for attr in event.attributes() {
        let attr = match attr {
            Ok(attr) => attr,
            Err(e) => Err(PyValueError::new_err(format!(
                "error decoding element attributes: {e:?}",
            )))?,
        };

        let value = match attr.unescape_value() {
            Ok(value) => value,
            Err(e) => Err(PyValueError::new_err(format!(
                "error decoding attribute value: {e:?}"
            )))?,
        };

        if xtype.is_none() && matches!(attr.key.prefix(), Some(n) if n.as_ref() == NAMESPACE_XSI) {
            xtype = Some(value);
        } else {
            match attr.key.prefix() {
                Some(p) if p.is_xml() => todo!("'xml:...' attributes are not implemented yet"),
                Some(p) if p.is_xmlns() => {
                    let key = String::from_utf8(p.into_inner().to_vec())?;
                    let value = String::from_utf8(attr.value.to_vec())?;
                    namespaces.insert(key, value);
                }
                Some(_) => Err(PyNotImplementedError::new_err(format!(
                    "namespaced attributes other than 'xsi:type' are not implemented yet: {:?}",
                    attr
                )))?,
                None => {
                    let key = attr.key.local_name().into_inner();
                    let key = String::from_utf8(key.to_vec())?;
                    attrs.insert(key, PyString::new(py, &value).unbind());
                }
            }
        }
    }

    if let Some(xtype) = xtype {
        let Some((nsalias, clsname)) = xtype.split_once(':') else {
            Err(PyNotImplementedError::new_err(format!(
                "'xsi:type' is not namespaced: {xtype:?}",
            )))?
        };
        let ns = Namespace::find(py, nsalias)?;
        let elm = ModelElement::new(ns, clsname, attrs.into_pyobject(py)?)?;
        let entry = model.id_index.entry(elm.id(py)?.to_string());
        use std::collections::hash_map::Entry as E;
        match entry {
            E::Occupied(mut entry) => {
                eprintln!("Duplicated ID: {}", entry.key());
                entry.insert(elm.clone_ref(py));
                model.mark_corrupt();
            }
            E::Vacant(entry) => {
                entry.insert(elm.clone_ref(py));
            }
        }
        Ok((namespaces, elm.into()))
    } else {
        let qn = event.name();
        let nsalias = qn.prefix().map(|_alias| todo!());
        let localname = string_cache.dedup(String::from_utf8(qn.local_name().as_ref().to_owned())?);
        let elm = XMLElement {
            tag: (nsalias, localname),
            text: None,
            attributes: attrs.into_iter().map(|(k, v)| (Arc::from(k), v)).collect(),
            children: Vec::new(),
        };
        Ok((namespaces, elm.into_pyobject(py)?.unbind().into()))
    }
}

fn finish_element(
    py: Python<'_>,
    model: &mut NativeLoader,
    parents: &[(HashMap<String, String>, AnyElement)],
    element: (HashMap<String, String>, AnyElement),
) -> PyResult<()> {
    todo!()
}

struct PyReader<'py> {
    file: Bound<'py, PyAny>,
}

impl<'py> PyReader<'py> {
    fn open(
        resources: &Bound<'py, PyDict>,
        resname: &str,
        filename: &Bound<'py, PyAny>,
    ) -> PyResult<Self> {
        let py = resources.py();
        let Some(res) = resources.get_item(resname)? else {
            let ecls = getclass(intern!(py, "MissingResourceError"));
            let resname = PyString::new(py, resname).unbind();
            Err(PyErr::from_type(ecls, (resname,)))?
        };
        let file = res.call_method1(intern!(py, "open"), (filename, intern!(py, "rb")))?;
        Ok(Self { file })
    }
}

impl<'py> std::io::Read for PyReader<'py> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let data = self
            .file
            .call_method1(intern!(self.file.py(), "read"), (buf.len(),))?
            .cast_into::<PyBytes>()
            .map_err(PyErr::from)?;
        let data = data.as_bytes();
        let retval = data.len();
        if retval > buf.len() {
            Err(PyRuntimeError::new_err(format!(
                "misbehaving FileHandler file: requested {} bytes, 'read()' returned {} bytes",
                buf.len(),
                retval,
            )))?
        }
        (buf[..data.len()]).copy_from_slice(&data);
        Ok(retval)
    }
}

enum AnyElement {
    ModelElement(ModelElement),
    XMLElement(Py<XMLElement>),
}

impl AnyElement {
    fn clone_ref(&self, py: Python<'_>) -> Self {
        match self {
            Self::ModelElement(e) => e.clone_ref(py).into(),
            Self::XMLElement(e) => e.clone_ref(py).into(),
        }
    }

    fn drop_ref(self, py: Python<'_>) {
        match self {
            Self::ModelElement(e) => e.drop_ref(py),
            Self::XMLElement(e) => e.drop_ref(py),
        }
    }
}

impl From<ModelElement> for AnyElement {
    fn from(value: ModelElement) -> Self {
        Self::ModelElement(value)
    }
}

impl From<Py<XMLElement>> for AnyElement {
    fn from(value: Py<XMLElement>) -> Self {
        Self::XMLElement(value)
    }
}

impl<'py> IntoPyObject<'py> for AnyElement {
    type Target = PyAny;
    type Output = Bound<'py, PyAny>;
    type Error = Infallible;

    fn into_pyobject(self, py: Python<'py>) -> Result<Self::Output, Self::Error> {
        match self {
            Self::ModelElement(el) => Ok(el.into_inner().into_bound(py)),
            Self::XMLElement(el) => Ok(el.into_any().into_bound(py)),
        }
    }
}

/// A generic XML element, which is not a model element.
#[pyclass]
struct XMLElement {
    tag: (Option<Arc<str>>, Arc<str>),
    text: Option<String>,
    attributes: HashMap<Arc<str>, Py<PyString>>,
    children: Vec<AnyElement>,
}

#[pymethods]
impl XMLElement {
    #[getter]
    fn get_tag(slf: PyRef<'_, Self>) -> Bound<'_, PyString> {
        if let Some(ref ns) = slf.tag.0 {
            PyString::new(slf.py(), &format!("{{{}}}{}", ns.as_ref(), slf.tag.1))
        } else {
            PyString::new(slf.py(), &slf.tag.1)
        }
    }

    fn __bool__(_slf: Py<Self>) -> bool {
        true
    }

    fn __len__(&self) -> usize {
        self.children.len()
    }

    fn __getitem__(slf: PyRef<'_, Self>, idx: usize) -> PyResult<AnyElement> {
        let py = slf.py();
        slf.children
            .get(idx)
            .map(|el| el.clone_ref(py))
            .ok_or_else(|| PyIndexError::new_err(idx))
    }

    #[pyo3(signature = (k, fallback = None))]
    fn get<'py>(
        &'py self,
        py: Python<'py>,
        k: &str,
        fallback: Option<Py<PyAny>>,
    ) -> Option<Py<PyAny>> {
        self.attributes
            .get(k)
            .map(|v| v.as_any().clone_ref(py))
            .or(fallback)
    }

    fn set<'py>(&'py mut self, k: String, v: Option<Py<PyString>>) {
        match v {
            None => self.attributes.remove(k.as_str()),
            Some(v) => self.attributes.insert(Arc::from(k), v),
        };
    }

    fn keys<'py>(&'py self) -> Vec<&'py str> {
        self.attributes.keys().map(|k| &**k).collect()
    }

    fn items<'py>(&'py self) -> Vec<(&'py str, &'py Py<PyString>)> {
        self.attributes.iter().map(|(k, v)| (&**k, v)).collect()
    }
}

#[derive(Default)]
struct StringCache(HashMap<String, Arc<str>>);

impl StringCache {
    fn dedup(&mut self, s: String) -> Arc<str> {
        self.0
            .entry(s)
            .or_insert_with_key(|k| Arc::from(k.as_str()))
            .clone()
    }
}
