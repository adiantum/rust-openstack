// Copyright 2018 Dmitry Tantsur <divius.inside@gmail.com>
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Flavor management via Compute API.

use std::collections::HashMap;
use std::rc::Rc;

use fallible_iterator::{FallibleIterator, IntoFallibleIterator};
use osproto::common::IdAndName;

use super::super::common::{FlavorRef, IntoVerified, Refresh, ResourceIterator, ResourceQuery};
use super::super::session::Session;
use super::super::utils::Query;
use super::super::{Error, Result};
use super::{api, protocol};

/// Structure representing a flavor.
#[derive(Clone, Debug)]
pub struct Flavor {
    session: Rc<Session>,
    inner: protocol::Flavor,
    extra_specs: HashMap<String, String>,
}

/// Structure representing a summary of a flavor.
#[derive(Clone, Debug)]
pub struct FlavorSummary {
    session: Rc<Session>,
    inner: IdAndName,
}

/// A query to flavor list.
#[derive(Clone, Debug)]
pub struct FlavorQuery {
    session: Rc<Session>,
    query: Query,
    can_paginate: bool,
}

/// A detailed query to flavor list.
#[derive(Clone, Debug)]
pub struct DetailedFlavorQuery {
    inner: FlavorQuery,
}

impl Flavor {
    /// Create a flavor object.
    pub(crate) fn new(session: Rc<Session>, mut inner: protocol::Flavor) -> Result<Flavor> {
        let extra_specs = match inner.extra_specs.take() {
            Some(es) => es,
            None => api::get_extra_specs_by_flavor_id(&session, &inner.id)?,
        };

        Ok(Flavor {
            session,
            inner,
            extra_specs,
        })
    }

    /// Load a Flavor object.
    pub(crate) fn load<Id: AsRef<str>>(session: Rc<Session>, id: Id) -> Result<Flavor> {
        let inner = api::get_flavor(&session, id)?;
        Flavor::new(session, inner)
    }

    /// Get ephemeral disk size in GiB.
    ///
    /// Returns `0` when ephemeral disk was not requested.
    pub fn emphemeral_size(&self) -> u64 {
        self.inner.ephemeral
    }

    /// Extra specs of the flavor.
    pub fn extra_specs(&self) -> &HashMap<String, String> {
        &self.extra_specs
    }

    /// Get a reference to flavor unique ID.
    pub fn id(&self) -> &String {
        &self.inner.id
    }

    /// Whether the flavor is public.
    pub fn is_public(&self) -> bool {
        self.inner.is_public
    }

    /// Get a reference to flavor name.
    pub fn name(&self) -> &String {
        &self.inner.name
    }

    /// Get RAM size in MiB.
    pub fn ram_size(&self) -> u64 {
        self.inner.ram
    }

    /// Get root disk size in GiB.
    pub fn root_size(&self) -> u64 {
        self.inner.disk
    }

    /// Get swap size in MiB.
    ///
    /// Returns `0` when swap was not requested.
    pub fn swap_size(&self) -> u64 {
        self.inner.swap
    }

    /// Get VCPU count.
    pub fn vcpu_count(&self) -> u32 {
        self.inner.vcpus
    }
}

impl Refresh for Flavor {
    /// Refresh the flavor.
    fn refresh(&mut self) -> Result<()> {
        self.inner = api::get_flavor_by_id(&self.session, &self.inner.id)?;
        Ok(())
    }
}

impl FlavorSummary {
    /// Get a reference to flavor unique ID.
    pub fn id(&self) -> &String {
        &self.inner.id
    }

    /// Get a reference to flavor name.
    pub fn name(&self) -> &String {
        &self.inner.name
    }

    /// Get details.
    pub fn details(&self) -> Result<Flavor> {
        Flavor::load(self.session.clone(), &self.inner.id)
    }
}

impl FlavorQuery {
    pub(crate) fn new(session: Rc<Session>) -> FlavorQuery {
        FlavorQuery {
            session,
            query: Query::new(),
            can_paginate: true,
        }
    }

    /// Add marker to the request.
    ///
    /// Using this disables automatic pagination.
    pub fn with_marker<T: Into<String>>(mut self, marker: T) -> Self {
        self.can_paginate = false;
        self.query.push_str("marker", marker);
        self
    }

    /// Add limit to the request.
    ///
    /// Using this disables automatic pagination.
    pub fn with_limit(mut self, limit: usize) -> Self {
        self.can_paginate = false;
        self.query.push("limit", limit);
        self
    }

    /// Convert this query into a detailed query.
    pub fn detailed(self) -> DetailedFlavorQuery {
        DetailedFlavorQuery { inner: self }
    }

    /// Convert this query into an iterator executing the request.
    ///
    /// This iterator yields only `FlavorSummary` objects, containing
    /// IDs and names. Use `into_iter_detailed` for full `Flavor` objects.
    ///
    /// Returns a `FallibleIterator`, which is an iterator with each `next`
    /// call returning a `Result`.
    ///
    /// Note that no requests are done until you start iterating.
    pub fn into_iter(self) -> ResourceIterator<FlavorQuery> {
        debug!("Fetching flavors with {:?}", self.query);
        ResourceIterator::new(self)
    }

    /// Execute this request and return all results.
    ///
    /// A convenience shortcut for `self.into_iter().collect()`.
    pub fn all(self) -> Result<Vec<FlavorSummary>> {
        self.into_iter().collect()
    }

    /// Return one and exactly one result.
    ///
    /// Fails with `ResourceNotFound` if the query produces no results and
    /// with `TooManyItems` if the query produces more than one result.
    pub fn one(mut self) -> Result<FlavorSummary> {
        debug!("Fetching one flavor with {:?}", self.query);
        if self.can_paginate {
            // We need only one result. We fetch maximum two to be able
            // to check if the query yieled more than one result.
            self.query.push("limit", 2);
        }

        self.into_iter().one()
    }
}

impl ResourceQuery for FlavorQuery {
    type Item = FlavorSummary;

    const DEFAULT_LIMIT: usize = 100;

    fn can_paginate(&self) -> Result<bool> {
        Ok(self.can_paginate)
    }

    fn extract_marker(&self, resource: &Self::Item) -> String {
        resource.id().clone()
    }

    fn fetch_chunk(&self, limit: Option<usize>, marker: Option<String>) -> Result<Vec<Self::Item>> {
        let query = self.query.with_marker_and_limit(limit, marker);
        Ok(api::list_flavors(&self.session, &query)?
            .into_iter()
            .map(|item| FlavorSummary {
                session: self.session.clone(),
                inner: item,
            })
            .collect())
    }
}

impl DetailedFlavorQuery {
    /// Convert this query into an iterator executing the request.
    ///
    /// This iterator yields full `Flavor` objects.
    ///
    /// Returns a `FallibleIterator`, which is an iterator with each `next`
    /// call returning a `Result`.
    ///
    /// Note that no requests are done until you start iterating.
    pub fn into_iter(self) -> ResourceIterator<DetailedFlavorQuery> {
        debug!("Fetching detailed flavors with {:?}", self.inner.query);
        ResourceIterator::new(self)
    }
}

impl ResourceQuery for DetailedFlavorQuery {
    type Item = Flavor;

    const DEFAULT_LIMIT: usize = 50;

    fn can_paginate(&self) -> Result<bool> {
        Ok(self.inner.can_paginate)
    }

    fn extract_marker(&self, resource: &Self::Item) -> String {
        resource.id().clone()
    }

    fn fetch_chunk(&self, limit: Option<usize>, marker: Option<String>) -> Result<Vec<Self::Item>> {
        let query = self.inner.query.with_marker_and_limit(limit, marker);
        let flavors = api::list_flavors_detail(&self.inner.session, &query)?;
        let mut result = Vec::with_capacity(flavors.len());
        for item in flavors {
            result.push(Flavor::new(self.inner.session.clone(), item)?);
        }
        Ok(result)
    }
}

impl IntoFallibleIterator for FlavorQuery {
    type Item = FlavorSummary;

    type Error = Error;

    type IntoFallibleIter = ResourceIterator<FlavorQuery>;

    fn into_fallible_iter(self) -> Self::IntoFallibleIter {
        self.into_iter()
    }
}

impl IntoFallibleIterator for DetailedFlavorQuery {
    type Item = Flavor;

    type Error = Error;

    type IntoFallibleIter = ResourceIterator<DetailedFlavorQuery>;

    fn into_fallible_iter(self) -> Self::IntoFallibleIter {
        self.into_iter()
    }
}

impl From<Flavor> for FlavorRef {
    fn from(value: Flavor) -> FlavorRef {
        FlavorRef::new_verified(value.inner.id)
    }
}

impl From<FlavorSummary> for FlavorRef {
    fn from(value: FlavorSummary) -> FlavorRef {
        FlavorRef::new_verified(value.inner.id)
    }
}

#[cfg(feature = "compute")]
impl IntoVerified for FlavorRef {
    /// Verify this reference and convert to an ID, if possible.
    fn into_verified(self, session: &Session) -> Result<FlavorRef> {
        Ok(if self.verified {
            self
        } else {
            FlavorRef::new_verified(api::get_flavor(session, &self.value)?.id)
        })
    }
}

impl From<Flavor> for protocol::ServerFlavor {
    fn from(value: Flavor) -> protocol::ServerFlavor {
        protocol::ServerFlavor {
            ephemeral_size: value.inner.ephemeral,
            extra_specs: Some(value.extra_specs),
            original_name: value.inner.name,
            ram_size: value.inner.ram,
            root_size: value.inner.disk,
            swap_size: value.inner.swap,
            vcpu_count: value.inner.vcpus,
        }
    }
}
