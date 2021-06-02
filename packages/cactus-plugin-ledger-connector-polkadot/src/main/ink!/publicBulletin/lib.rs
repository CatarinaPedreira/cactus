#![cfg_attr(not(feature = "std"), no_std)]
#![feature(in_band_lifetimes)]

use ink_lang as ink;

#[ink::contract(dynamic_storage_allocator = true)]
mod public_bulletin {

    /// A commitment is a (View, RollingHash) tuple, where a View represents a permissioned blockchain's state at a given height, and a RollingHash contains the history of past Views
    type Commitment = (String, String);

    use std::collections::hash_map::DefaultHasher;
    use std::hash::Hasher;
    use ink_storage::traits::PackedLayout;
    use ink_env::AccountId;
    use ink_storage::{
        collections::HashMap as HashMap,
        alloc::Box as StorageBox,
        collections::Vec as Vec,
    };

    #[ink(event)]
    /// Emitted when a view is published
    pub struct ViewPublished {
        height: i32,
        member: AccountId,
        view: String,
    }

    #[ink(event)]
    /// Emitted when there is no consensus on a view
    pub struct ViewConflict {
        height: i32,
        view: String,
        rolling_hash: String,
    }

    #[ink(event)]
    /// Emitted when there is a view that needs to be approved
    pub struct ViewApprovalRequest {
        height: i32,
        member: AccountId,
        view: String,
        rolling_hash: String,
    }

    #[ink(storage)]
    /// Contains the storage of the PublicBulletin
    pub struct PublicBulletin {
        /// Each member is associated with several heights, and each height is associated with a commitment
        commitments_per_member: HashMap<AccountId, StorageBox<HashMap<i32, Commitment>>>,
        /// Account ID's which correspond to the committee members of the blockchain
        whitelist: Vec<AccountId>
    }

    impl PublicBulletin {

        #[ink(constructor)]
        pub fn default() -> Self {
            Self {
                commitments_per_member: HashMap::new(),
                whitelist: Vec::new()
            }
        }

        #[ink(message)]
        pub fn get_commitments_per_member(&self) -> &HashMap<AccountId, StorageBox<HashMap<i32, Commitment>>> {
            &self.commitments_per_member
        }

        #[ink(message)]
        pub fn get_whitelist(&self) -> &Vec<AccountId> {
            &self.whitelist
        }

        /// Publish a given commitment (on a given height) in the public bulletin and announce it to the network
        #[ink(message)]
        pub fn publish_view(&mut self, height: i32, member: &AccountId, view: &String, rolling_hash: &String) {
            // Check if the account that wants to publish a view is actually a member of the permissioned blockchain
            if self.check_contains(&self.whitelist, member) {
                let mut published = false;
                let mut commitments = self.get_all_commitments(height);

                // In case the member does not yet belong to the commitments_per_member hashmap, add the corresponding entry
                if !(self.commitments_per_member.contains_key(member)) {
                    let new_commitments = StorageBox::new(HashMap::new());
                    self.commitments_per_member.insert((*member).clone(), new_commitments);
                }

                // Check if this view already exists in the commitments of other members, for the given height
                for commitment in commitments {
                    if (*view == commitment.0) && (self.calculate_rolling_hash(height.clone(), member) == rolling_hash) {
                        // If the view already exists in this height (published by a different member), it means that it is valid, so it can be published right away
                        self.add_and_publish_view(height.clone(), member, view, rolling_hash);
                        published = true;
                        break;
                    }
                }

                // In case the view is new, it has to be approved by the member committee to be published
                if !published && self.approve_view(height.clone(), member, view, rolling_hash) {
                    self.add_and_publish_view(height.clone(), member, view, rolling_hash);
                }
            }
        }

        /// Approve a commitment or rise a conflict for it, depending on the committee members' evaluation
        fn approve_view(&self, height: i32, member: &AccountId, view: &String, rolling_hash: &String) -> bool {
            let replies: Vec<String> = Vec::new();

            // Emit event to request the committee members to approve a commitment with a given height and member
            self.env().emit_event(ViewApprovalRequest {
                height,
                member: (*member).clone(),
                view: (*view).clone(),
                rolling_hash: (*rolling_hash).clone(),
            });

            // TODO Missing receiving replies and putting them on replies vector

            // If all members approve the view, it will be published
            if !self.check_contains(&replies, &String::from("NOK")) {
                true
            }
            else {
                // If at least one member does not approve the view, a view conflict arises and it's not published
                self.report_conflict(height, view, rolling_hash);
                false
            }
        }

        /// Report a conflict for a given commitment
        #[ink(message)]
        pub fn report_conflict(&self, height: i32, view: &String, rolling_hash: &String) {
            self.env().emit_event(ViewConflict {
                height,
                view: (*view).clone(),
                rolling_hash: (*rolling_hash).clone(),
            });
        }

        /// Aux: Check if vector contains a given element
        fn check_contains<T: PackedLayout + PartialEq>(&self, vec: &Vec<T>, element: &T) -> bool {
            let mut res = false;
            for acc in vec.iter(){
                if acc == element {
                    res = true;
                }
            }
            res
        }

        /// Aux: Retrieve all commitments for a given height
        fn get_all_commitments(&self, height: i32) -> &Vec<Commitment> {
            let mut result: Vec<Commitment> = Vec::new();
            for (_, map) in self.commitments_per_member.iter() {
                let entry = map.get(&height);
                if entry.is_some() {
                    result.push(*(entry.unwrap()));
                }
            }
            &result
        }

        /// Add a commitment to the Public Bulletin and emit an event to announce this to the network
        fn add_and_publish_view(&mut self, height: i32, member: &AccountId, view: &String, rolling_hash: &String) {
            self.commitments_per_member.get_mut(member).unwrap().insert(height, ((*view).clone(), (*rolling_hash).clone()));
            self.env().emit_event(ViewPublished {
                height,
                member: (*member).clone(),
                view: (*view).clone(),
            });
        }

        /// Calculate the rolling hash for a given height and member
        fn calculate_rolling_hash(&self, height: i32, member: &AccountId) -> &str {
            // Formula for rolling_hash: H(i) = hash(hash(V_(i-1)) || hash(H_(i-l1)))

            let previous_commitment_opt = self.commitments_per_member.get(member).unwrap().get(&(height-1));
            let res: &str;

            // The rolling hash will only be calculated in case the member has a commitment for the previous height
            if previous_commitment_opt.is_some() {
                let previous_commitment = previous_commitment_opt.unwrap();
                let previous_view: &str = &previous_commitment.0;
                let previous_rolling_hash: &str = &previous_commitment.1;

                let mut hasher_view = DefaultHasher::new();
                hasher_view.write(previous_view.as_bytes());
                hasher_view.finish();

                let mut hasher_roll = DefaultHasher::new();
                hasher_roll.write(previous_rolling_hash.as_bytes());
                hasher_roll.finish();

                let formatted_res: &str = &format!("{}{}", previous_view, previous_rolling_hash);
                let mut hasher_res = DefaultHasher::new();
                hasher_res.write(formatted_res.as_bytes());
                res = &format!("{:x}", hasher_res.finish());

            // Otherwise, the rolling hash will be a simple "None" string
            } else {
                res = "None";
            }
            res
        }

    }


    // TODO finish tests !!
    #[cfg(test)]
    mod tests {
        /// Imports all the definitions from the outer scope so we can use them here.
        use super::*;

        /// Imports `ink_lang` so we can use `#[ink::test]`.
        use ink_lang as ink;

        /// We test if the default constructor does its job.
        #[ink::test]
        fn default_works() {
            let public_bulletin_sc = PublicBulletin::default();
            //public_bulletin_sc.get_whitelist();
            //public_bulletin_sc.get_commitments_per_member();
        }
    }
}
