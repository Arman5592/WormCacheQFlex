// BSD 3-Clause License
//
// Copyright (c) 2024, Parallel Systems Architecture Laboratory (PARSA), EPFL.
// All rights reserved.
//
// Redistribution and use in source and binary forms, with or without
// modification, are permitted provided that the following conditions are met:
//
// 1. Redistributions of source code must retain the above copyright notice, this
//    list of conditions and the following disclaimer.
//
// 2. Redistributions in binary form must reproduce the above copyright notice,
//    this list of conditions and the following disclaimer in the documentation
//    and/or other materials provided with the distribution.
//
// 3. Neither the name of the PARSA, EPFL
//    nor the names of its contributors may be used to endorse or promote
//    products derived from this software without specific prior written
//    permission.
//
// THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS"
// AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE
// IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
// DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE
// FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL
// DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
// SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER
// CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY,
// OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE
// OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

use proc_macro::TokenStream;
use quote::quote;
use syn::{Data, DeriveInput, Fields, parse_macro_input};

#[proc_macro_derive(PluginHelper)]
pub fn plugin_helper(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);

    // Extract the identifier (name) of the struct
    let name = input.ident;

    // Initialize a variable to hold generated code for each field's dd call
    let mut on_translation_calls = Vec::new();
    let mut init_calls = Vec::new();
    let mut serialize_calls = Vec::new();
    let mut deserialize_calls = Vec::new();

    // Match against the data of the struct to access its fields
    if let Data::Struct(data) = input.data {
        if let Fields::Named(fields) = data.fields {
            // Iterate over the struct's fields
            for field in fields.named {
                let ty = &field.ty;
                // Assuming the field type implements DD, generate a dd call
                on_translation_calls.push(quote! {
                    #ty::on_translation(tb);
                });
                init_calls.push(quote! {
                    #ty::init(plugin_id, options);
                });
                serialize_calls.push(quote! {
                    #ty::serialize(name);
                });
                deserialize_calls.push(quote! {
                    #ty::deserialize(name);
                });
            }
        }
    }

    // Generate the implementation of the dd function for the struct
    let expanded = quote! {
        impl #name {
            #[inline]
            pub unsafe fn on_translation(tb: *mut crate::qemu_api::qemu_plugin_tb) {
                #( #on_translation_calls )*
            }

            #[inline]
            pub unsafe fn init(plugin_id: u64, options: &FxHashMap<String, String>) {
                #( #init_calls )*
            }

            #[inline]
            pub unsafe fn serialize(name: &str) {
                #( #serialize_calls )*
            }

            #[inline]
            pub unsafe fn deserialize(name: &str) {
                #( #deserialize_calls )*
            }
        }
    };

    TokenStream::from(expanded)
}
