// Copyright (C) 2019-2022 Aleo Systems Inc.
// This file is part of the snarkVM library.

// The snarkVM library is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// The snarkVM library is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with the snarkVM library. If not, see <https://www.gnu.org/licenses/>.

use crate::{Address, Network, ViewKey};
use snarkvm_circuits_environment::Mode;
use snarkvm_curves::{AffineCurve, ProjectiveCurve};
use snarkvm_fields::PrimeField;
use snarkvm_utilities::{FromBits, ToBits};

use anyhow::{bail, Result};
use itertools::Itertools;

/// A general purpose data structure for representing program data in a record.
pub trait DataType: Clone + ToBits + FromBits {}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Data<N: Network, D: DataType> {
    /// Publicly-visible data.
    Plaintext(D, Mode),
    /// Private data encrypted under the account owner's address.
    Ciphertext(Vec<N::Field>, Mode),
}

impl<N: Network, D: DataType> Data<N, D> {
    /// Returns the mode of the data.
    pub const fn mode(&self) -> Mode {
        match self {
            Self::Plaintext(_, mode) => *mode,
            Self::Ciphertext(_, mode) => *mode,
        }
    }

    /// Returns `true` if the enum variant corresponds to the correct mode.
    /// Otherwise, the method returns `false`.
    pub fn is_valid(&self) -> bool {
        match self {
            Self::Plaintext(_, mode) => mode.is_constant() || mode.is_public(),
            Self::Ciphertext(_, mode) => mode.is_private(),
        }
    }

    /// Returns the data ID.
    pub fn to_data_id(&self) -> Result<N::Field> {
        match self.is_valid() {
            true => match self {
                Self::Plaintext(data, _) => N::hash_psd8(&Self::encode(data)?),
                Self::Ciphertext(data, _) => N::hash_psd8(data),
            },
            false => bail!("Failed to compute the data ID as the data must be encrypted first"),
        }
    }

    /// Encrypts `self` under the given Aleo address and randomizer,
    /// turning `self` into `Data::Ciphertext(..)` if the `mode` is private.
    /// Note: The output is guaranteed to satisfy `Data::is_valid(output)`.
    pub fn encrypt(&self, address: Address<N>, randomizer: N::Scalar) -> Result<Self> {
        match self {
            Self::Plaintext(data, Mode::Private) => {
                // Encode the data as field elements.
                let plaintext = Self::encode(data)?;
                // Compute the data view key.
                let data_view_key = (*address * randomizer).to_affine().to_x_coordinate();
                // Prepare a randomizer for each field element.
                let randomizers = N::hash_many_psd8(&[N::encryption_domain(), data_view_key], plaintext.len());
                // Compute the ciphertext field elements.
                let ciphertext = plaintext.iter().zip_eq(randomizers).map(|(p, r)| *p + r).collect();
                // Output the ciphertext.
                Ok(Self::Ciphertext(ciphertext, Mode::Private))
            }
            _ => Ok((*self).clone()),
        }
    }

    /// Decrypts `self` into plaintext using the given view key & nonce,
    /// turning `Data::Ciphertext(..)` into `Data::Plaintext(..)`.
    /// Note: The output does **not** necessarily satisfy `Data::is_valid(output)`.
    pub fn decrypt(&self, view_key: ViewKey<N>, nonce: N::Affine) -> Result<Self> {
        match self {
            Self::Plaintext(..) => Ok((*self).clone()),
            Self::Ciphertext(ciphertext, mode) => {
                // Compute the data view key.
                let data_view_key = (nonce.to_projective() * *view_key).to_affine().to_x_coordinate();
                // Prepare a randomizer for each field element.
                let randomizers = N::hash_many_psd8(&[N::encryption_domain(), data_view_key], ciphertext.len());
                // Compute the plaintext field elements.
                let plaintext: Vec<_> = ciphertext.iter().zip_eq(randomizers).map(|(c, r)| *c - r).collect();
                // Decode the data from field elements, and output the plaintext.
                Ok(Self::Plaintext(Self::decode(&plaintext), *mode))
            }
        }
    }
}

impl<N: Network, D: DataType> Data<N, D> {
    /// Returns a list of field elements encoding the given data.
    pub(super) fn encode(data: &D) -> Result<Vec<N::Field>> {
        // Encode the data as little-endian bits.
        let mut bits = data.to_bits_le();
        // Adds one final bit to the data, to serve as a terminus indicator.
        // During decryption, this final bit ensures we've reached the end.
        bits.push(true);
        // Pack the bits into field elements.
        bits.chunks(N::Field::size_in_data_bits())
            .map(|bits| {
                // Recover the base field.
                match N::Field::from_repr(<N::Field as PrimeField>::BigInteger::from_bits_le(bits)) {
                    // We know this case will always work, because we truncate the output to CAPACITY bits in the base field.
                    Some(field) => Ok(field),
                    _ => bail!("Failed to encode data bits into a base field"),
                }
            })
            .collect()
    }

    /// Returns the recovered data from encoded field elements.
    pub(super) fn decode(plaintext: &[N::Field]) -> D {
        // Unpack the field elements into bits, and reverse the list to pop the terminus bit off.
        let mut bits = plaintext.iter().flat_map(|p| p.to_bits_le()[..N::Field::size_in_data_bits()].to_vec()).rev();
        // Remove the terminus bit that was added during encoding.
        for boolean in bits.by_ref() {
            // Drop all extraneous `0` bits, in addition to the final `1` bit.
            if boolean {
                // This case will always be reached, since the terminus bit is always `1`.
                break;
            }
        }
        // Reverse the bits back and recover the data from the bits.
        D::from_bits_le(&bits.rev().collect::<Vec<_>>())
    }
}

// #[cfg(test)]
// mod tests {
//     use super::*;
//     use crate::{PrivateKey, Testnet3};
//     use snarkvm_utilities::{test_crypto_rng, FromBytes, Rng, UniformRand, ToBytes};
//
//     use core::ops::AddAssign;
//
//     type CurrentNetwork = Testnet3;
//
//     pub const ITERATIONS: usize = 1000;
//
//     #[test]
//     fn test_encrypt_and_decrypt() -> Result<()> {
//         // Generate an address, view key, and private key.
//         let private_key = PrivateKey::<CurrentNetwork>::new(&mut test_crypto_rng())?;
//         let view_key = ViewKey::try_from(&private_key)?;
//         let address = Address::try_from(&private_key)?;
//
//         // Generate a random plaintext data.
//         let message = (0..1024).map(|_| rand::random::<u8>()).collect::<Vec<u8>>();
//         let plaintext = Data::Plaintext(message, Mode::Private);
//         assert!(!plaintext.is_valid());
//
//         // Encrypt the data.
//         let randomizer = UniformRand::rand(&mut test_crypto_rng());
//         let ciphertext = plaintext.encrypt(address, randomizer)?;
//         assert!(ciphertext.is_valid());
//
//         // Decrypt the data.
//         let candidate = ciphertext.decrypt(view_key, CurrentNetwork::g_scalar_multiply(&randomizer))?;
//         assert_eq!(plaintext, candidate);
//
//         Ok(())
//     }
//
// //     #[test]
// //     fn test_encryption_symmetric_key_commitment() -> Result<()> {
// //         // Generate an address, view key, and private key.
// //         let private_key = PrivateKey::<CurrentNetwork>::new(&mut test_crypto_rng())?;
// //         let view_key = ViewKey::try_from(&private_key)?;
// //         let address = Address::try_from(&private_key)?;
// //
// //         let (_randomness, ciphertext_randomizer, symmetric_key) = encryption.generate_asymmetric_key(&public_key, rng);
// //         let symmetric_key_commitment = encryption.generate_symmetric_key_commitment(&symmetric_key);
// //
// //         {
// //             // Sanity check that the symmetric key matches, when derived from the private key.
// //             let candidate_symmetric_key =
// //                 encryption.generate_symmetric_key(&private_key, ciphertext_randomizer).unwrap();
// //             assert_eq!(symmetric_key, candidate_symmetric_key);
// //         }
// //         {
// //             // Sanity check that the symmetric key commitment is deterministic.
// //             let candidate_symmetric_key_commitment = encryption.generate_symmetric_key_commitment(&symmetric_key);
// //             assert_eq!(symmetric_key_commitment, candidate_symmetric_key_commitment);
// //         }
// //
// //         // Ensure different symmetric keys for the same public key fail to match the symmetric key commitment.
// //         for _ in 0..ITERATIONS {
// //             let (_randomness, _ciphertext_randomizer, alternate_symmetric_key) =
// //                 encryption.generate_asymmetric_key(&public_key, rng);
// //             let candidate_symmetric_key_commitment =
// //                 encryption.generate_symmetric_key_commitment(&alternate_symmetric_key);
// //             assert_ne!(symmetric_key_commitment, candidate_symmetric_key_commitment);
// //         }
// //
// //         // Ensure different private keys fail to match the symmetric key commitment.
// //         for _ in 0..ITERATIONS {
// //             let alternate_private_key = encryption.generate_private_key(rng);
// //             let alternate_public_key = encryption.generate_public_key(&alternate_private_key);
// //             let (_randomness, _ciphertext_randomizer, alternate_symmetric_key) =
// //                 encryption.generate_asymmetric_key(&alternate_public_key, rng);
// //             let candidate_symmetric_key_commitment =
// //                 encryption.generate_symmetric_key_commitment(&alternate_symmetric_key);
// //             assert_ne!(symmetric_key_commitment, candidate_symmetric_key_commitment);
// //         }
// //
// //         Ok(())
// //     }
// //
// //     #[test]
// //     fn test_ciphertext_random_manipulation() -> Result<()> {
// //         // Generate an address, view key, and private key.
// //         let private_key = PrivateKey::<CurrentNetwork>::new(&mut test_crypto_rng())?;
// //         let view_key = ViewKey::try_from(&private_key)?;
// //         let address = Address::try_from(&private_key)?;
// //
// //         let (_randomness, _ciphertext_randomizer, symmetric_key) = encryption.generate_asymmetric_key(&address, rng);
// //
// //         let number_of_bytes = 320;
// //         let message = (0..number_of_bytes).map(|_| rand::random::<u8>()).collect::<Vec<u8>>();
// //         let encoded_message = TestEncryptionScheme::encode_message(&message).unwrap();
// //         let ciphertext = encryption.encrypt(&symmetric_key, &encoded_message);
// //         dbg!(ciphertext.len());
// //
// //         let candidate_message = encryption.decrypt(&symmetric_key, &ciphertext);
// //         let decoded_message = TestEncryptionScheme::decode_message(&candidate_message).unwrap();
// //         assert_eq!(message, decoded_message);
// //
// //         // Ensure any mutation fails to match the original message.
// //         for _ in 0..ITERATIONS {
// //             // Copy the ciphertext.
// //             let mut ciphertext = ciphertext.clone();
// //
// //             // Mutate one of the ciphertext elements.
// //             let x = rng.gen_range(0..5);
// //             ciphertext[x].add_assign(Fq::one());
// //
// //             // This should fail.
// //             let candidate_message = encryption.decrypt(&symmetric_key, &ciphertext);
// //             let decoded_message = TestEncryptionScheme::decode_message(&candidate_message).unwrap();
// //             assert_ne!(message, decoded_message);
// //         }
// //
// //         Ok(())
// //     }
// }
