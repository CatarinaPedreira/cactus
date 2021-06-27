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
        member: InkAccountId,
        view: String,
        rolling_hash: String,
    }

    #[ink(event)]
    /// Emitted when there is a view that needs to be approved
    pub struct ViewApprovalRequest {
        height: i32,
        member: InkAccountId,
        view: String,
    }

    #[ink(storage)]
    /// Contains the storage of the PublicBulletin
    pub struct PublicBulletin {
        /// Each member is associated with several heights, and each height is associated with a commitment
        commitments_per_member: HashMap<InkAccountId, StorageBox<HashMap<i32, Commitment>>>,
        /// Each member is associated with several heights, and each height is associated with the evaluations of other members for that commitment
        replies_per_member: HashMap<InkAccountId, StorageBox<HashMap<i32, StorageBox<Vec<String>>>>>,
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

        // TODO change InkAccountId::from([0x1; 32]) to InkAccountId::default() and test accordingly
        /// Add a committee member to this contract
        #[ink(message)]
        pub fn add_member(&mut self, member: &InkAccountId) {
            let caller: InkAccountId = self.env().caller();
            if caller == InkAccountId::from([0x1; 32]) && !(self.check_contains(&self.whitelist, member)) {
                self.whitelist.push((*member).clone());
                self.commitments_per_member.insert((*member).clone(), StorageBox::new(HashMap::new()));
                self.replies_per_member.insert((*member).clone(), StorageBox::new(HashMap::new()));
            }
        }

        // TODO change InkAccountId::from([0x1; 32]) to InkAccountId::default() and test accordingly
        /// Remove a member from this contract
        #[ink(message)]
        pub fn remove_member(&mut self, member: &InkAccountId) {
            let caller: InkAccountId = self.env().caller();
            if caller == InkAccountId::from([0x1; 32]) && self.check_contains(&self.whitelist, member) {
                let index = self.whitelist.iter().position(|x| x == member).unwrap();
                self.whitelist.swap_remove_drop(index as u32);
                self.commitments_per_member.take(member);
                self.replies_per_member.take(member);
            }
        }

        /// Publish a given commitment (on a given height) in the public bulletin and announce it to the network
        #[ink(message)]
        pub fn publish_view(&mut self, height: i32, view: &String, rolling_hash: &String) {
            // Check if the account that wants to publish a view is actually a member of the permissioned blockchain
            let caller: &InkAccountId = &self.env().caller();
            if self.check_contains(&self.whitelist, caller) {
                // Check if a view for this member and height already exists
                if !(self.commitments_per_member.get(caller).unwrap().get(&height).is_some()) {
                    let mut published = false;
                    let commitments = self.get_all_commitments(height);

                    // Check if this view already exists in the commitments of other members, for the given height
                    for commitment in commitments.iter() {
                        // If the view already exists in this height (published by a different member), it means that it is valid, so it can be published right away
                        if *view == commitment.0 {
                            // If the provided rolling hash is correct, we add and publish the view
                            if &(self.calculate_rolling_hash(height.clone())) == rolling_hash {
                                self.add_and_publish_view(height.clone(), view, rolling_hash);
                                published = true;
                                break;
                                // Else, the view is not published and a conflict arises
                            } else {
                                return self.report_conflict(height.clone(), view, rolling_hash);
                            }
                        }
                    }

                    // In case the view is new, it has to be approved by the member committee to be published
                    if !published && self.approve_view(height.clone(), view, rolling_hash) && &(self.calculate_rolling_hash(height.clone())) == rolling_hash {
                        self.add_and_publish_view(height.clone(), view, rolling_hash);
                    }
                }
            }
        }

        /// Approve a commitment or rise a conflict for it, depending on the committee members' evaluation
        fn approve_view(&mut self, height: i32, view: &String, rolling_hash: &String) -> bool {
            let caller: InkAccountId = self.env().caller();
            let member_replies = self.replies_per_member.get_mut(&caller).unwrap();

            // Initialize reply vector for this height and member
            if member_replies.get(&height).is_none() {
                member_replies.insert(height, StorageBox::new(Vec::new()));
            }

            // Emit event to request the committee members to approve a commitment with a given height and member
            self.env().emit_event(ViewApprovalRequest {
                height: height.clone(),
                member: caller,
                view: (*view).clone(),
            });

            // Block thread until getting a number of replies equal to the size of the quorum for the current committee members.
            // We trust that we will always have at least this amount of replies, hence that this loop will never be infinite
            while !(self.get_all_replies(&height).len() == self.calculate_quorum()) {}

            // After getting all replies, if all members approve the view, it will be published
            if !self.check_contains(&self.get_all_replies(&height), &String::from("NOK")) {
                true
            }
            else {
                // If at least one member does not approve the view, a view conflict arises and it's not published
                self.report_conflict(height.clone(), view, rolling_hash);
                false
            }
        }

        /// A committee member calls this function to approve or reject a given view
        fn evaluate_view(&mut self, height: i32, evaluated_member: &InkAccountId, verdict: String) {
            // Check if the account that wants to evaluate a view is actually a member of the blockchain
            if self.check_contains(&self.whitelist, &self.env().caller()) {
                self.replies_per_member.get_mut(evaluated_member).unwrap().get_mut(&height).unwrap().push(verdict);
            }
        }

        /// Report a conflict for a given commitment
        #[ink(message)]
        pub fn report_conflict(&self, height: i32, view: &String, rolling_hash: &String) {
            let caller: InkAccountId = self.env().caller();
            // Check if this account is actually a member of the blockchain
            if self.check_contains(&self.whitelist, &caller) {
                self.env().emit_event(ViewConflict {
                    height,
                    member: caller,
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

        /// Aux: Retrieve the commitments of all members for a given height
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

        /// Aux: Retrieve all replies for this account, for a given height
        fn get_all_replies(&self, height: &i32) -> Vec<String> {
            let caller: &InkAccountId = &self.env().caller();
            let mut replies: Vec<String> = Vec::new();
            let map_option = self.replies_per_member.get(caller).unwrap().get(height);
            if map_option.is_some() {
                for str in map_option.unwrap().iter() {
                    replies.push(str.to_string());
                }
            }
            replies
        }

        /// Aux: Calculate quorum according to current committee members
        fn calculate_quorum(&self) -> u32 {
            let res = self.whitelist.len() / 2;
            if self.whitelist.len() <= 2 {
                res
            } else {
                res + 1
            }
        }

        /// Aux: Add a commitment to the Public Bulletin and emit an event to announce this to the network
        fn add_and_publish_view(&mut self, height: i32, view: &String, rolling_hash: &String) {
            let caller: InkAccountId = self.env().caller();
            self.commitments_per_member.get_mut(&caller).unwrap().insert(height, ((*view).clone(), (*rolling_hash).clone()));
            self.env().emit_event(ViewPublished {
                height: height.clone(),
                member: caller,
                view: (*view).clone(),
            });
        }

        /// Aux: Calculate the rolling hash for a given height and member
        fn calculate_rolling_hash(&self, height: i32) -> String {
            // Formula for rolling_hash: H(i) = hash(hash(V_(i-1)) || hash(H_(i-l1)))
            let caller: &InkAccountId = &self.env().caller();
            let previous_commitment_opt = self.commitments_per_member.get(caller).unwrap().get(&(height-1));
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

        // ******************************* Functions for test usage ********************************

        /// Aux, get contract owner
        fn get_owner(&self) -> InkAccountId {
            (self.owner).clone()
        }

        /// Aux: Get wrapped commitment of this account for a given height
        fn get_wrapped_commitment(&self, height: i32) -> Option<&Commitment> {
            let caller: &InkAccountId = &self.env().caller();
            let option = self.commitments_per_member.get(caller);
            if option.is_none() {
                None
            } else {
                option.unwrap().get(&height)
            }
        }

        /// Aux: Add a commitment manually, without having to publish it
        fn add_commitment_manually(&mut self, height: i32, member: &InkAccountId, view: &String, rolling_hash: &String) {
            self.commitments_per_member.get_mut(member).unwrap().insert(height, (view.to_string(), rolling_hash.to_string()));
        }

        /// Aux: Add a new height to the replies_per_member hashmap
        fn add_height_to_replies(&mut self, height: i32) {
            let caller: &InkAccountId = &self.env().caller();
            self.replies_per_member.get_mut(caller).unwrap().insert(height, StorageBox::new(Vec::new()));
        }

    }

    #[cfg(test)]
    mod tests {
        /// Imports definitions from the outer scope so we can use them here.
        use super::*;

        use ink_lang as ink;
        use ink_env::AccountId;

        #[ink::test]
        fn whitelist_works() {
            let mut public_bulletin_sc = PublicBulletin::default();
            public_bulletin_sc.add_member(&public_bulletin_sc.get_owner());
            public_bulletin_sc.publish_view(1, &String::from("TryAddView"), &String::from("None"));
            assert_eq!(*(public_bulletin_sc.get_wrapped_commitment(1).unwrap()), (String::from("TryAddView"), String::from("None")));
            public_bulletin_sc.remove_member(&public_bulletin_sc.get_owner());
            public_bulletin_sc.publish_view(2, &String::from("TryAddOtherView"), &String::from("None"));
            assert_eq!(public_bulletin_sc.get_wrapped_commitment(2), None);
        }

        #[ink::test]
        fn pre_existing_view_correct_hash() {
            let mut public_bulletin_sc = PublicBulletin::default();
            public_bulletin_sc.add_member(&public_bulletin_sc.get_owner());
            public_bulletin_sc.add_member(&InkAccountId::from([0x2; 32]));
            public_bulletin_sc.add_commitment_manually(1, &InkAccountId::from([0x2; 32]), &String::from("TestEqualViews"), &String::from("None"));
            public_bulletin_sc.publish_view(1, &String::from("TestEqualViews"), &String::from("None"));

            assert_eq!(*(public_bulletin_sc.get_wrapped_commitment(1).unwrap()), (String::from("TestEqualViews"), String::from("None")));
        }


        #[ink::test]
        fn pre_existing_view_incorrect_hash() {
            let mut public_bulletin_sc = PublicBulletin::default();
            public_bulletin_sc.add_member(&public_bulletin_sc.get_owner());
            public_bulletin_sc.add_member(&InkAccountId::from([0x2; 32]));
            public_bulletin_sc.add_commitment_manually(1, &InkAccountId::from([0x2; 32]), &String::from("TestEqualViews"), &String::from("None"));
            public_bulletin_sc.publish_view(1, &String::from("TestEqualViews"), &String::from("WrongHash"));

            assert_eq!(public_bulletin_sc.get_wrapped_commitment(1), None);
        }


        #[ink::test]
        fn new_view_one_member() {
            let mut public_bulletin_sc = PublicBulletin::default();
            public_bulletin_sc.add_member(&public_bulletin_sc.get_owner());
            public_bulletin_sc.publish_view(1,&String::from("View"), &String::from("None"));

            assert_eq!(*(public_bulletin_sc.get_wrapped_commitment(1).unwrap()), (String::from("View"), String::from("None")));
        }

        #[ink::test]
        fn new_view_two_approvals(){
            let mut public_bulletin_sc = PublicBulletin::default();
            let mut reply_vec = Vec::new();
            reply_vec.push(String::from("OK"));
            reply_vec.push(String::from("Ok"));

            public_bulletin_sc.add_member(&public_bulletin_sc.get_owner());
            public_bulletin_sc.add_member(&AccountId::from([0x2; 32]));
            public_bulletin_sc.add_member(&AccountId::from([0x3; 32]));
            public_bulletin_sc.add_height_to_replies(1);

            // Note: Once the contract is deployed, the three functions below will not be called sequentially.
            // When the 'publish_view()' function is called, it will announce a view approval request and then concurrently
            // wait for a given number of calls to the 'evaluate_view()' function before it proceeds.
            public_bulletin_sc.evaluate_view(1, &public_bulletin_sc.get_owner(), String::from("OK"));
            public_bulletin_sc.evaluate_view(1, &public_bulletin_sc.get_owner(), String::from("OK"));
            public_bulletin_sc.publish_view(1, &String::from("View"), &String::from("None"));

            assert_eq!(*(public_bulletin_sc.get_wrapped_commitment(1).unwrap()), (String::from("View"), String::from("None")));
        }

        //
        #[ink::test]
        fn new_view_approval_and_disapproval(){
            let mut public_bulletin_sc = PublicBulletin::default();
            let mut reply_vec = Vec::new();
            reply_vec.push(String::from("OK"));
            reply_vec.push(String::from("Ok"));

            public_bulletin_sc.add_member(&public_bulletin_sc.get_owner());
            public_bulletin_sc.add_member(&AccountId::from([0x2; 32]));
            public_bulletin_sc.add_member(&AccountId::from([0x3; 32]));
            public_bulletin_sc.add_height_to_replies(1);

            // Note: Once the contract is deployed, the three functions below will not be called sequentially.
            // When the 'publish_view()' function is called, it will announce a view approval request and then concurrently
            // wait for a given number of calls to the 'evaluate_view()' function before it proceeds.
            public_bulletin_sc.evaluate_view(1, &public_bulletin_sc.get_owner(), String::from("OK"));
            public_bulletin_sc.evaluate_view(1, &public_bulletin_sc.get_owner(), String::from("NOK"));
            public_bulletin_sc.publish_view(1, &String::from("View"), &String::from("None"));

            assert_eq!(public_bulletin_sc.get_wrapped_commitment(1), None);
        }

        #[ink::test]
        fn two_views_correct_hash() {
            let mut public_bulletin_sc = PublicBulletin::default();
            public_bulletin_sc.add_member(&public_bulletin_sc.get_owner());
            public_bulletin_sc.publish_view(1, &String::from("View1"), &String::from("None"));
            public_bulletin_sc.publish_view(2, &String::from("View2"), &String::from("6d8694a1e486efa9"));

            assert_eq!(*(public_bulletin_sc.get_wrapped_commitment(1).unwrap()), (String::from("View1"), String::from("None")));
            assert_eq!(*(public_bulletin_sc.get_wrapped_commitment(2).unwrap()), (String::from("View2"), String::from("6d8694a1e486efa9")));
        }

        #[ink::test]
        fn two_views_incorrect_hash() {
            let mut public_bulletin_sc = PublicBulletin::default();
            public_bulletin_sc.add_member(&public_bulletin_sc.get_owner());
            public_bulletin_sc.publish_view(1, &String::from("View1"), &String::from("None"));
            public_bulletin_sc.publish_view(2, &String::from("View2"), &String::from("IncorrectHash"));

            assert_eq!(*(public_bulletin_sc.get_wrapped_commitment(1).unwrap()), (String::from("View1"), String::from("None")));
            assert_eq!(public_bulletin_sc.get_wrapped_commitment(2), None);
        }

        #[ink::test]
        fn two_commitments_same_height_and_member() {
            let mut public_bulletin_sc = PublicBulletin::default();
            public_bulletin_sc.add_member(&public_bulletin_sc.get_owner());
            public_bulletin_sc.publish_view(1, &String::from("FirstView"), &String::from("None"));
            public_bulletin_sc.publish_view(1, &String::from("TryReplaceFirst"), &String::from("None"));

            assert_eq!(*(public_bulletin_sc.get_wrapped_commitment(1).unwrap()), (String::from("FirstView"), String::from("None")));
        }
    }
}
