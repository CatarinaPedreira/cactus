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
    use ink_storage::{collections::HashMap as HashMap, alloc::Box as StorageBox, collections::Vec as Vec};
    use ink_env::{AccountId as InkAccountId};

    #[ink(event)]
    /// Emitted when a view is published
    pub struct ViewPublished {
        height: i32,
        member: InkAccountId,
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
        member: InkAccountId,
        view: String,
        rolling_hash: String,
    }

    #[ink(storage)]
    /// Contains the storage of the PublicBulletin
    pub struct PublicBulletin {
        /// Each member is associated with several heights, and each height is associated with a commitment
        commitments_per_member: HashMap<InkAccountId, StorageBox<HashMap<i32, Commitment>>>,
        /// Each members' approval or disapproval of each heights' commitments
        replies_per_member: HashMap<InkAccountId, StorageBox<HashMap<i32, String>>>,
        /// Account ID's which correspond to the committee members of the blockchain
        whitelist: Vec<InkAccountId>,
        /// The account of the owner of this contract
        owner: InkAccountId
    }

    impl PublicBulletin {

        #[ink(constructor)]
        pub fn default() -> Self {
            Self {
                commitments_per_member: HashMap::new(),
                replies_per_member: HashMap::new(),
                whitelist: Vec::new(),
                owner: Self::env().caller()
            }
        }

        /// Add a committee member to this contract
        #[ink(message)]
        pub fn add_member(&mut self, member: &InkAccountId) {
            if !(self.check_contains(&self.whitelist, member)) {
                self.whitelist.push(*member);
                self.commitments_per_member.insert(*member, StorageBox::new(HashMap::new()));
                self.replies_per_member.insert(*member, StorageBox::new(HashMap::new()));
            }
        }

        /// Publish a given commitment (on a given height) in the public bulletin and announce it to the network
        #[ink(message)]
        pub fn publish_view(&mut self, height: i32, member: &InkAccountId, view: &String, rolling_hash: &String) {
            // Check if the account that wants to publish a view is actually a member of the permissioned blockchain
            if self.check_contains(&self.whitelist, member) {
                let mut published = false;
                let commitments = self.get_all_commitments(height);

                // Check if this view already exists in the commitments of other members, for the given height
                for commitment in commitments.iter() {
                    // If the view already exists in this height (published by a different member), it means that it is valid, so it can be published right away
                    if *view == commitment.0 {
                        // If the provided rolling hash is correct, we add and publish the view
                        if &(self.calculate_rolling_hash(height.clone(), member)) == rolling_hash {
                            self.add_and_publish_view(height.clone(), member, view, rolling_hash);
                            published = true;
                            break;
                        // Else, the view is not published and a conflict arises
                        } else {
                            return self.report_conflict(height, member, view, rolling_hash);
                        }
                    }
                }

                // In case the view is new, it has to be approved by the member committee to be published
                if !published && self.approve_view(height.clone(), member, view, rolling_hash) {
                    self.add_and_publish_view(height.clone(), member, view, rolling_hash);
                }
            }
        }

        /// Approve a commitment or rise a conflict for it, depending on the committee members' evaluation
        fn approve_view(&self, height: i32, member: &InkAccountId, view: &String, rolling_hash: &String) -> bool {

            // Emit event to request the committee members to approve a commitment with a given height and member
            self.env().emit_event(ViewApprovalRequest {
                height: height.clone(),
                member: (*member).clone(),
                view: (*view).clone(),
                rolling_hash: (*rolling_hash).clone(),
            });

            // Block thread until getting a number of replies equal to the size of the quorum for the current committee members.
            // We trust that we will always have at least this amount of replies, hence that this loop will never be infinite
            while !(self.get_all_replies(height).len() == self.calculate_quorum()) {}

            // After getting all replies, if all members approve the view, it will be published
            if !self.check_contains(&self.get_all_replies(height), &String::from("NOK")) {
                true
            }
            else {
                // If at least one member does not approve the view, a view conflict arises and it's not published
                self.report_conflict(height, member, view, rolling_hash);
                false
            }
        }

        /// A committee member calls this function to approve or reject a given view
        #[ink(message)]
        pub fn evaluate_view(&mut self, height: i32, member: &InkAccountId, verdict: String){
            // Check if the account that wants to evaluate a view is actually a member of the blockchain
            if self.check_contains(&self.whitelist, member) {
                // Add reply to the corresponding member and height
                self.replies_per_member.get_mut(member).unwrap().insert(height, verdict);
            }
        }

        /// Report a conflict for a given commitment
        #[ink(message)]
        pub fn report_conflict(&self, height: i32, member: &InkAccountId, view: &String, rolling_hash: &String) {
            // Check if the account that wants to report conflict on a view is actually a member of the blockchain
            if self.check_contains(&self.whitelist, member) {
                self.env().emit_event(ViewConflict {
                    height,
                    view: (*view).clone(),
                    rolling_hash: (*rolling_hash).clone(),
                });
            }
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
        fn get_all_commitments(&self, height: i32) -> Vec<Commitment> {
            let mut result: Vec<Commitment> = Vec::new();
            for (_, map) in self.commitments_per_member.iter() {
                let entry = map.get(&height);
                if entry.is_some() {
                    // If the commitment is not none, push it to result vector
                    result.push((entry.unwrap().0.clone(), entry.unwrap().1.clone()));
                }
            }
            result
        }

        /// Aux: Retrieve all replies for a given height
        fn get_all_replies(&self, height: i32) -> Vec<String> {
            let mut result: Vec<String> = Vec::new();
            for (_, map) in self.replies_per_member.iter() {
                let entry = map.get(&height);
                if entry.is_some() {
                    // If the reply is not none, push it to result vector
                    result.push(entry.unwrap().to_string());
                }
            }
            result
        }

        /// Aux: Calculate quorum according to current committee members
        fn calculate_quorum(&self) -> u32 {
            (self.whitelist.len() / 2) + 1
        }

        /// Aux: Add a commitment to the Public Bulletin and emit an event to announce this to the network
        fn add_and_publish_view(&mut self, height: i32, member: &InkAccountId, view: &String, rolling_hash: &String) {
            self.commitments_per_member.get_mut(member).unwrap().insert(height, ((*view).clone(), (*rolling_hash).clone()));
            self.env().emit_event(ViewPublished {
                height: height.clone(),
                member: (*member).clone(),
                view: (*view).clone(),
            });
        }

        /// Aux: Calculate the rolling hash for a given height and member
        fn calculate_rolling_hash(&self, height: i32, member: &InkAccountId) -> String {
            // Formula for rolling_hash: H(i) = hash(hash(V_(i-1)) || hash(H_(i-l1)))

            let previous_commitment_opt = self.commitments_per_member.get(member).unwrap().get(&(height-1));
            let res: String;

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
                res = format!("{:x}", hasher_res.finish());

            // Otherwise, the rolling hash will be a simple "None" string
            } else {
                res = String::from("None");
            }
            res
        }


        // ********************************* Functions for tests **********************************

        fn get_owner(&self) -> InkAccountId {
            self.owner
        }

        /// Aux: Get wrapped commitment for given height
        fn get_wrapped_commitment(&self, height: i32, member: &InkAccountId) -> Option<&Commitment> {
            let option = self.commitments_per_member.get(member);
            if option.is_none() {
                None
            } else {
                option.unwrap().get(&height)
            }
        }

        fn add_commitment_manually(&mut self, height: i32, member: &InkAccountId, view: &String, rolling_hash: &String) {
            self.commitments_per_member.get_mut(member).unwrap().insert(height, (view.to_string(), rolling_hash.to_string()));
        }

    }

    #[cfg(test)]
    mod tests {
        /// Imports all the definitions from the outer scope so we can use them here.
        use super::*;

        /// Imports `ink_lang` so we can use `#[ink::test]`.
        use ink_lang as ink;

        /// We first test if the whitelist works, only allowing authorized members, that need to be added to the list, to perform actions.
        #[ink::test]
        fn whitelist_works() {
            let mut public_bulletin_sc = PublicBulletin::default();
            public_bulletin_sc.publish_view(1, &public_bulletin_sc.get_owner(), &String::from("TryAddView"), &String::from("None"));
            assert_eq!(public_bulletin_sc.get_wrapped_commitment(1, &public_bulletin_sc.get_owner()), None);
        }

        #[ink::test]
        fn equal_views() {
            let mut public_bulletin_sc = PublicBulletin::default();
            public_bulletin_sc.add_member(&InkAccountId::default());
            public_bulletin_sc.add_member(&public_bulletin_sc.get_owner());
            public_bulletin_sc.add_commitment_manually(1, &InkAccountId::default(), &String::from("TestEqualViews"), &String::from("None"));
            public_bulletin_sc.publish_view(1, &public_bulletin_sc.get_owner(), &String::from("TestEqualViews"), &String::from("None"));
            assert_eq!(*(public_bulletin_sc.get_wrapped_commitment(1, &InkAccountId::default()).unwrap()), (String::from("TestEqualViews"), String::from("None")));
            assert_eq!(*(public_bulletin_sc.get_wrapped_commitment(1, &public_bulletin_sc.get_owner()).unwrap()), (String::from("TestEqualViews"), String::from("None")));
        }

        #[ink::test]
        fn equal_views_wrong_hash() {
            let mut public_bulletin_sc = PublicBulletin::default();
            public_bulletin_sc.add_member(&InkAccountId::default());
            public_bulletin_sc.add_member(&public_bulletin_sc.get_owner());
            public_bulletin_sc.add_commitment_manually(1, &InkAccountId::default(), &String::from("TestEqualViews"), &String::from("None"));
            public_bulletin_sc.publish_view(1, &public_bulletin_sc.get_owner(), &String::from("TestEqualViews"), &String::from("WrongHash"));
            assert_eq!(*(public_bulletin_sc.get_wrapped_commitment(1, &InkAccountId::default()).unwrap()), (String::from("TestEqualViews"), String::from("None")));
            assert_eq!(public_bulletin_sc.get_wrapped_commitment(1, &public_bulletin_sc.get_owner()), None);
        }

        // TODO missing tests related to the approve_view and evaluate_view functions

    }
}
