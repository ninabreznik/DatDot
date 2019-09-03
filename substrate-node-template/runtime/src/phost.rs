/// A runtime module template with necessary imports

/// Feel free to remove or edit this file as needed.
/// If you change the name of this file, make sure to update its references in runtime/src/lib.rs
/// If you remove this file, you can remove those references


/// For more guidance on Substrate modules, see the example module
/// https://github.com/paritytech/substrate/blob/master/srml/example/src/lib.rs

use support::{
	decl_module,
	decl_storage, 
	decl_event, 
	ensure,
	StorageValue, 
	StorageMap, 
	dispatch::Result
};
use core::convert::TryInto;
use system::ensure_signed;
use codec::{Encode, Decode};
use rstd::vec::Vec;
use primitives::{ed25519, Hasher, Blake2Hasher, H256};
use runtime_io::ed25519_verify;

type Public = ed25519::Public;
type Signature = ed25519::Signature;

/// The module's configuration trait.
pub trait Trait: system::Trait {
	type Event: From<Event<Self>> + Into<<Self as system::Trait>::Event>;
}

#[derive(Decode, PartialEq, Eq, Encode, Clone)]
#[cfg_attr(feature = "std", derive(Debug))]
pub struct Node {
	index: u64,
	hash: H256,
	size: u64
}

#[derive(Decode, PartialEq, Eq, Encode, Clone)]
#[cfg_attr(feature = "std", derive(Debug))]
pub struct Proof {
	index: u64,
	nodes: Vec<Node>,
	signature: Option<Signature>
}

type DatIdIndex = u64;
type DatIdVec = Vec<DatIdIndex>;
type UserIdIndex = u64;
// FIXME this is a naive way to approximate size/cost  
type DatSize = u64;

decl_event!(
	pub enum Event<T> 
	where
	AccountId = <T as system::Trait>::AccountId,
	BlockNumber = <T as system::Trait>::BlockNumber 
	{
		SomethingStored(DatIdIndex, Public),
		SomethingUnstored(DatIdIndex, Public),
		Challenge(AccountId,BlockNumber),
		NewPin(AccountId, Public),
	}
);

// This module's storage items.
decl_storage! {
	trait Store for Module<T: Trait> as TemplateModule {
		// A vec of free indeces, with the last item usable for `len`
		DatId get(next_id): DatIdVec;
		// Each dat archive has a public key
		DatKey get(public_key): map DatIdIndex => Public;
		// Each dat archive has a tree size
		TreeSize get(tree_size): map Public => DatSize;
		// each dat archive has a merkle root
		MerkleRoot get(merkle_root): map Public => (H256, Signature);
		// users are put into an "array"
		UsersCount: u64;
		Users get(user): map UserIdIndex => T::AccountId;
		// each user has a vec of dats they seed
		UsersStorage: map T::AccountId => Vec<Public>;
		// each dat has a vecc of users pinning it
		DatHosters: map Public => Vec<T::AccountId>;
		// each user has a mapping and vec of dats they want seeded
		UserRequestsMap: map Public => T::AccountId;

		// current check condition
		SelectedUser: T::AccountId;
		// Dat and which index to verify
		SelectedDat: (Public, u64);
		TimeLimit get(time_limit): T::BlockNumber;
		Nonce: u64;
	}
}


// The module's dispatchable functions.
decl_module! {
	/// The module declaration.
	pub struct Module<T: Trait> for enum Call where origin: T::Origin {
		fn deposit_event<T>() = default;

		fn on_initialize(n: T::BlockNumber) {
			let dat_vec = <DatId>::get();
			match dat_vec.last() {
				Some(last_index) => {
			// if no one is currently selected to give proof, select someone
			if !<SelectedUser<T>>::exists() && <UsersCount>::get() > 0 {
				let nonce = <Nonce>::get();
				let new_random = (<system::Module<T>>::random_seed(), nonce)
					.using_encoded(|b| Blake2Hasher::hash(b))
					.using_encoded(|mut b| u64::decode(&mut b))
					.expect("Hash must be bigger than 8 bytes; Qed");
				let new_time_limit = new_random % last_index;
				let future_block = 
					n + T::BlockNumber::from(new_time_limit.try_into().unwrap_or(2));
				let random_user_index = new_random % <UsersCount>::get();
				let random_user = <Users<T>>::get(random_user_index);
				let users_dats = <UsersStorage<T>>::get(&random_user);
				let users_dats_len = users_dats.len();
				let random_dat = users_dats.get(new_random as usize % users_dats_len)
					.expect("Users must not have empty storage; Qed");
				let dat_tree_len = <TreeSize>::get(random_dat);
				let random_leave = new_random % dat_tree_len;
				<SelectedDat>::put((random_dat.clone(), random_leave));
				<SelectedUser<T>>::put(&random_user);
				<TimeLimit<T>>::put(future_block);
				<Nonce>::mutate(|m| *m += 1);
				Self::deposit_event(RawEvent::Challenge(random_user, future_block));
			}},
				None => (),
			}
		}

		fn submit_proof(origin, proof: Proof) {
			let account = ensure_signed(origin)?;
			ensure!(
				account == <SelectedUser<T>>::get(),
				"Only the current challengee can respond to their challenge"
			);
			let index_proved = proof.index;
			let challenge = <SelectedDat>::get();
			ensure!(
				index_proved == challenge.1,
				"Proof is for the wrong chunk"
			);

			// if proof okay (TODO: EPIC: HARD: DIFFICULT)
				//charge archive pinner (FUTURE: scale by some burn_factor)
				//reward and unselect prover
			<SelectedUser<T>>::kill();
			// else let the user try again until time limit
		}

		// Submit or update a piece of data that you want to have users copy
		fn register_data(origin, merkle_root: (Public, H256, Signature), tree_size: DatSize) {
			let account = ensure_signed(origin)?;
			let pubkey = merkle_root.0;
			let mut lowest_free_index : DatIdIndex = 0;
			//FIXME: we don't currently verify if we are updating to a newer root from an older one.
			//FIXME: we don't currently verify tree_size
			//verify the signature
			ensure!(
				ed25519_verify(
					&merkle_root.2,
					&merkle_root.1.as_bytes(),
					&pubkey
					),
				"Signature Verification Failed"
			);
			let mut dat_vec : Vec<DatIdIndex> = <DatId>::get();
			if !<MerkleRoot>::exists(&pubkey){
				match dat_vec.first() {
					Some(index) => {
						lowest_free_index = dat_vec.remove(0);
						if dat_vec.is_empty() {
							dat_vec.push(lowest_free_index + 1);
						}
					},
					None => {
						//add an element if the vec is empty
						dat_vec.push(1);
					},
				}
				//register new unknown dats
				//TODO: charge users for doing this the first time
				<DatKey>::insert(&lowest_free_index, &pubkey)
			}
			<MerkleRoot>::insert(&pubkey, (merkle_root.1, merkle_root.2));
			<DatId>::put(dat_vec);
			<TreeSize>::insert(&pubkey, tree_size);
			<UserRequestsMap<T>>::insert(&pubkey, &account);

			//TODO: charge users based on tree size in regular intervals in on_finalize()
			//FUTURE: everything should be scaled and prioritized by some burn_factor, 
			Self::deposit_event(RawEvent::SomethingStored(lowest_free_index, pubkey))
		}

		//user stops requesting others pin their data
		fn unregister_data(origin, index: DatIdIndex){
			let account = ensure_signed(origin)?;
			let pubkey = <DatKey>::get(index);
			//only allow owner to unregister
			ensure!(
				<UserRequestsMap<T>>::get(&pubkey) == account,
				"Cannot unregister archive, not owner."
			);
			let mut dat_vec = <DatId>::get();
			match dat_vec.last() {
				Some(last_index) => {
					if index == last_index - 1 {
						dat_vec.pop();
					}
					dat_vec.push(index);
					dat_vec.sort_unstable();
					dat_vec.dedup();
					<DatId>::put(dat_vec);
				},
				None => (), //should never happen!
			}
			<DatHosters<T>>::get(&pubkey)
				.iter()
				.for_each(|account| {
					let mut storage : Vec<Public> =
						<UsersStorage<T>>::get(account.clone());
					match storage.binary_search(&pubkey).ok() {
						Some(index) => {storage.remove(index);},
						None => (),
					}
					<UsersStorage<T>>::insert(account.clone(), &storage);
				});
			<DatHosters<T>>::remove(&pubkey);
			<DatKey>::remove(&index);
			<UserRequestsMap<T>>::remove(&pubkey);
			<TreeSize>::remove(&pubkey);
			<MerkleRoot>::remove(&pubkey);
			//if the dat being unregistered is currently part of the challenge
			if (<SelectedDat>::get().0 == pubkey){
				<SelectedUser<T>>::kill();
			}
			Self::deposit_event(RawEvent::SomethingUnstored(index, pubkey));
		}

		// User requests a dat for them to pin. FIXME: May return a dat they are already pinning.
		fn register_backup(origin) {
			//TODO: bias towards unseeded dats and burn_factor
			let account = ensure_signed(origin)?;
			let dat_vec = <DatId>::get();
			match dat_vec.last() {
				Some(last_index) => {
				let nonce = <Nonce>::get();
				let new_random = (<system::Module<T>>::random_seed(), &nonce, &account)
					.using_encoded(|b| Blake2Hasher::hash(b))
					.using_encoded(|mut b| u64::decode(&mut b))
					.expect("Hash must be bigger than 8 bytes; Qed");
				let random_index = new_random % last_index;
				let dat_pubkey = DatKey::get(random_index);
				let mut current_user_dats = <UsersStorage<T>>::get(&account);
				let mut dat_hosters = <DatHosters<T>>::get(&dat_pubkey);
				current_user_dats.push(dat_pubkey.clone());
				current_user_dats.sort_unstable();
				current_user_dats.dedup();
				dat_hosters.push(account.clone());
				dat_hosters.sort_unstable();
				dat_hosters.dedup();
				<DatHosters<T>>::insert(&dat_pubkey, &dat_hosters);
				<UsersStorage<T>>::insert(&account, &current_user_dats);
				<Nonce>::mutate(|m| *m += 1);
				if(current_user_dats.len() == 1){
					<Users<T>>::insert(<UsersCount>::get(), &account);
					<UsersCount>::mutate(|m| *m += 1);
				}
				Self::deposit_event(RawEvent::NewPin(account, dat_pubkey));
				},
				None => (),
			}
		}

		fn on_finalize(n: T::BlockNumber) {
			if (n == Self::time_limit()) {
				let user = <SelectedUser<T>>::take();
				//(todo) calculate some punishment
				//(todo) punish user
				//currently we only remove user from future challenges
				<UsersStorage<T>>::remove(user);
				<UsersCount>::mutate(|m| *m -= 1);
			}
		}
	}
}

/// tests for this module
#[cfg(test)]
mod tests {
	use super::*;

	use runtime_io::with_externalities;
	use primitives::{H256, Blake2Hasher};
	use support::{impl_outer_origin, assert_ok, parameter_types};
	use sr_primitives::{traits::{BlakeTwo256, IdentityLookup}, testing::Header};
	use sr_primitives::weights::Weight;
	use sr_primitives::Perbill;

	impl_outer_origin! {
		pub enum Origin for Test {}
	}

	// For testing the module, we construct most of a mock runtime. This means
	// first constructing a configuration type (`Test`) which `impl`s each of the
	// configuration traits of modules we want to use.
	#[derive(Clone, Eq, PartialEq)]
	pub struct Test;
	parameter_types! {
		pub const BlockHashCount: u64 = 250;
		pub const MaximumBlockWeight: Weight = 1024;
		pub const MaximumBlockLength: u32 = 2 * 1024;
		pub const AvailableBlockRatio: Perbill = Perbill::from_percent(75);
	}
	impl system::Trait for Test {
		type Origin = Origin;
		type Call = ();
		type Index = u64;
		type BlockNumber = u64;
		type Hash = H256;
		type Hashing = BlakeTwo256;
		type AccountId = u64;
		type Lookup = IdentityLookup<Self::AccountId>;
		type Header = Header;
		type WeightMultiplierUpdate = ();
		type Event = ();
		type BlockHashCount = BlockHashCount;
		type MaximumBlockWeight = MaximumBlockWeight;
		type MaximumBlockLength = MaximumBlockLength;
		type AvailableBlockRatio = AvailableBlockRatio;
		type Version = ();
	}
	impl Trait for Test {
		type Event = ();
	}
	type TemplateModule = Module<Test>;

	// This function basically just builds a genesis storage key/value store according to
	// our desired mockup.
	fn new_test_ext() -> runtime_io::TestExternalities<Blake2Hasher> {
		system::GenesisConfig::default().build_storage::<Test>().unwrap().into()
	}

	#[test]
	fn it_works_for_default_value() {
		with_externalities(&mut new_test_ext(), || {
			// Just a dummy test for the dummy funtion `do_something`
			// calling the `do_something` function with a value 42
			assert_ok!(TemplateModule::do_something(Origin::signed(1), 42));
			// asserting that the stored value is equal to what we stored
			assert_eq!(TemplateModule::something(), Some(42));
		});
	}
}
