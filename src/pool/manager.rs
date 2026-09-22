//-----------------------------------------------------------------------------
// Copyright (c) 2026, Oracle and/or its affiliates.
//
// This software is dual-licensed to you under the Universal Permissive License
// (UPL) 1.0 as shown at https://oss.oracle.com/licenses/upl and Apache License
// 2.0 as shown at http://www.apache.org/licenses/LICENSE-2.0. You may choose
// either license.
//
// If you elect to accept the software under the Apache License, Version 2.0,
// the following applies:
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//    https://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//-----------------------------------------------------------------------------

//-----------------------------------------------------------------------------
// manager.rs
//
// Defines the structure that manages the contents of the pool.
//-----------------------------------------------------------------------------

use std::sync::mpsc;

use crate::config::PoolConfig;
use crate::connection::ConnImpl;
use crate::error::Error;

use super::PoolContentsRef;

pub(super) struct PoolManager {
    config: PoolConfig,
    receive_channel: mpsc::Receiver<PoolManagerRequest>,
    contents_ref: PoolContentsRef,
}

pub(super) enum PoolManagerRequest {
    GrowPool,
    ClosePool,
    DropConnection(ConnImpl),
    PingConnection(ConnImpl),
}

impl PoolManager {
    /// Grows the pool by creating a connection and adding it to the pool.
    fn grow_pool(&self) {
        let result = ConnImpl::connect(
            self.config.connection_config().clone(),
            self.config.pool_id().into(),
        );
        self.contents_ref.lock().unwrap().add_new_connection(result);
    }

    /// Pings a connection and indicates whether or not it is safe to coninue
    /// using.
    fn ping_connection(
        &self,
        conn_impl: ConnImpl,
    ) -> Result<ConnImpl, (ConnImpl, Error)> {
        let orig_call_timeout = match conn_impl.get_call_timeout() {
            Ok(timeout) => timeout,
            Err(err) => return Err((conn_impl, err)),
        };
        if let Err(err) =
            conn_impl.set_call_timeout(Some(self.config.ping_timeout()))
        {
            return Err((conn_impl, err));
        }
        let ping_result = conn_impl.ping();
        let restore_result = conn_impl.set_call_timeout(orig_call_timeout);
        match ping_result {
            Ok(()) => match restore_result {
                Ok(()) => Ok(conn_impl),
                Err(err) => Err((conn_impl, err)),
            },
            Err(err) => Err((conn_impl, err)),
        }
    }

    /// Creates a new pool manager.
    pub(super) fn new(
        contents_ref: PoolContentsRef,
        receive_channel: mpsc::Receiver<PoolManagerRequest>,
        config: PoolConfig,
    ) -> PoolManager {
        PoolManager {
            config,
            receive_channel,
            contents_ref,
        }
    }

    /// Runs the manager (in a thread).
    pub(super) fn run(&mut self) {
        for request in &self.receive_channel {
            match request {
                PoolManagerRequest::GrowPool => {
                    self.grow_pool();
                }
                PoolManagerRequest::ClosePool => {
                    break;
                }
                PoolManagerRequest::DropConnection(mut conn_impl) => {
                    let _ = conn_impl.close();
                }
                PoolManagerRequest::PingConnection(conn_impl) => {
                    let result = match self.ping_connection(conn_impl) {
                        Ok(conn_impl) => Some(conn_impl),
                        Err((mut conn_impl, _err)) => {
                            conn_impl.discard();
                            None
                        }
                    };
                    self.contents_ref
                        .lock()
                        .unwrap()
                        .add_pinged_connection(result);
                }
            }
        }
    }
}
