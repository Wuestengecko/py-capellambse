// SPDX-FileCopyrightText: Copyright DB InfraGO AG
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::{HashMap, VecDeque},
    io::BufReader,
    sync::Arc,
};

use pyo3::{
    exceptions::{PyNotImplementedError, PyOSError, PyRuntimeError, PyValueError},
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
                let elm = parse_element(py, model, &mut stack, ev)?;
                stack.push(elm);
            }
            Ok(E::Empty(ev)) => {
                let elm = parse_element(py, model, &stack, ev)?;
                finish_element(py, model, &stack, elm)?;
            }
            Ok(E::End(_)) => {
                let elm = stack.pop().unwrap();
                finish_element(py, model, &stack, elm)?;
            }
            Ok(E::Text(ev)) => match stack
                .last_mut()
                .ok_or_else(|| PyValueError::new_err("orphaned text at document root?"))?
            {
                AnyElement::ModelElement(elm) => Err(PyValueError::new_err(format!(
                    "unhandled text directly within element {}",
                    elm.id(py)
                        .map(|id| id.to_string_lossy().to_string())
                        .unwrap_or_else(|_| "<unknown id>".into())
                )))?,
                AnyElement::XMLElement(elm) => {
                    elm.borrow_mut(py).text = Some(ev.xml_content()?.to_string())
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
    parents: &[AnyElement],
    event: BytesStart<'_>,
) -> PyResult<AnyElement> {
    let attrs = PyDict::new(py);
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

        if xtype.is_none() && attr.key.prefix().as_ref().map(|n| n.as_ref()) == Some(NAMESPACE_XSI)
        {
            xtype = Some(value);
        } else {
            let key = match attr.key.prefix() {
                Some(p) if p.is_xml() || p.is_xmlns() => Err(PyNotImplementedError::new_err(
                    "'xml:...' and 'xmlns:...' attributes are not implemented yet",
                ))?,
                Some(_) => Err(PyNotImplementedError::new_err(
                    "namespaced attributes other than 'xsi:type' are not implemented yet",
                ))?,
                None => attr.key.local_name().into_inner(),
            };
            attrs.set_item(key, value)?;
        }
    }

    if let Some(xtype) = xtype {
        let Some((nsalias, clsname)) = xtype.split_once(':') else {
            Err(PyNotImplementedError::new_err(format!(
                "'xsi:type' is not namespaced: {xtype:?}",
            )))?
        };
        let ns = Namespace::find(py, nsalias)?;
        let elm = ModelElement::new(ns, clsname, attrs)?;
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
        Ok(elm.into())
    } else {
        Ok(None)
    }
}

fn finish_element(
    py: Python<'_>,
    model: &mut NativeLoader,
    parents: &[AnyElement],
    element: AnyElement,
) -> PyResult<()> {
    todo!()
}

struct PyReader<'py> {
    file: Bound<'py, PyAny>,
    buf: VecDeque<u8>,
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
        Ok(Self {
            file,
            buf: VecDeque::new(),
        })
    }
}

impl<'py> std::io::Read for PyReader<'py> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let data = self
            .file
            .call_method1(intern!(self.file.py(), "read"), (buf.len(),))?
            .downcast_into::<PyBytes>()
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

/// A generic XML element, which is not a model element.
#[pyclass]
struct XMLElement {
    tag: Arc<str>,
    text: Option<String>,
    attributes: HashMap<Arc<str>, String>,
    children: Vec<AnyElement>,
}

#[pymethods]
impl XMLElement {
    #[pyo3(signature = (k, fallback = None))]
    fn get<'py>(
        &'py self,
        py: Python<'py>,
        k: &str,
        fallback: Option<Py<PyAny>>,
    ) -> Option<Py<PyAny>> {
        self.attributes
            .get(k)
            .map(|v| PyString::new(py, v).into_any().unbind())
            .or(fallback)
    }

    fn set<'py>(&'py mut self, k: String, v: Option<String>) {
        match v {
            None => self.attributes.remove(k.as_str()),
            Some(v) => self.attributes.insert(Arc::from(k), v),
        };
    }

    fn keys<'py>(&'py self) -> Vec<&'py str> {
        self.attributes.keys().map(|k| &**k).collect()
    }

    fn items<'py>(&'py self) -> Vec<(&'py str, &'py str)> {
        self.attributes
            .iter()
            .map(|(k, v)| (&**k, v.as_str()))
            .collect()
    }
}
