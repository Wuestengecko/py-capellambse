// SPDX-FileCopyrightText: Copyright DB InfraGO AG
// SPDX-License-Identifier: Apache-2.0

use std::{collections::VecDeque, io::BufReader};

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
                if let Some(elm) = elm {
                    stack.push(elm.into());
                } else {
                    stack.push(
                        match stack.last().ok_or_else(|| {
                            PyValueError::new_err("non-element tag at document root")
                        })? {
                            _ => (),
                        }
                        .clone_ref(py),
                    )
                }
            }
            Ok(E::Empty(ev)) => {
                if let Some(elm) = parse_element(py, model, &mut stack, ev)? {
                    elm.drop_ref(py);
                }
            }
            Ok(E::End(_)) => match stack.pop().unwrap() {
                Parent::ModelElement(elm) => elm.drop_ref(py),
                _ => (),
            },
            Ok(E::Text(ev)) => todo!("encountered Text in the XML"),
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

fn parse_element<'a>(
    py: Python<'_>,
    model: &mut NativeLoader,
    parents: &[Parent],
    event: BytesStart<'a>,
) -> PyResult<Option<ModelElement>> {
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
        Ok(Some(elm))
    } else {
        Ok(None)
    }
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

enum Parent {
    ModelElement(ModelElement),
    XMLElement(XMLElement),
}

impl From<ModelElement> for Parent {
    fn from(value: ModelElement) -> Self {
        Self::ModelElement(value)
    }
}

impl From<XMLElement> for Parent {
    fn from(value: XMLElement) -> Self {
        Self::XMLElement(value)
    }
}
