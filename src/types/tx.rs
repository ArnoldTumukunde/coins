use std::io::{Read, Write};
use bitcoin_spv::types::{Hash256Digest};

use crate::{
    hashes::{
        hash256::Hash256Writer,
        marked::{MarkedHash, TXID, WTXID},
    },
    types::{
        primitives::{Ser, TxError, TxResult, PrefixVec},
        txin::{TxIn, Vin},
        txout::{TxOut, Vout},
        script::{Script, Witness},
    },
};

#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Sighash{
    All = 0x01,
    None = 0x02,
    Single = 0x3,
    AllACP = 0x81,
    NoneACP = 0x82,
    SingleACP = 0x83,
}

trait Transaction<'a>: Ser {
   type TxIn;
   type TxOut;
   type SighashArgs;
   type TXID: MarkedHash<Hash256Digest>;
   // TODO: abstract the hash writer

   fn inputs(&'a self) -> &'a[Self::TxIn];
   fn outputs(&'a self) -> &'a[Self::TxOut];
   fn version(&self) -> u32;
   fn locktime(&self) -> u32;

   fn txid(&self) -> Self::TXID;

   fn write_legacy_sighash_preimage<W: Write>(&self, writer: &mut W, _args: &Self::SighashArgs) -> TxResult<()>;

   fn legacy_sighash(&self, args: &Self::SighashArgs) -> Hash256Digest {
       let mut w = Hash256Writer::default();
       self.write_legacy_sighash_preimage(&mut w, args).expect("No IOError from SHA2");
       w.finish()
   }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LegacySighashArgs<'a> {
    /// The index of the input we'd like to sign
    pub index: usize,
    /// The sighash mode to use.
    pub sighash_flag: Sighash,
    /// The script used in the prevout, which must be signed.
    pub prevout_script: &'a Script,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LegacyTx {
    version: u32,
    vin: Vin,
    vout: Vout,
    locktime: u32
}

impl LegacyTx {
    fn legacy_sighash_prep(&self, index: usize, prevout_script: &Script) -> Self
    {
        let mut copy_tx = self.clone();

        for i in 0..copy_tx.vin.len() {
            copy_tx.vin[i].script_sig = if i == index {
                prevout_script.clone()
            } else {
                Script::null()
            };
        };
        copy_tx
    }

    fn legacy_sighash_single(
        copy_tx: &mut Self,
        index: usize) -> TxResult<()>
    {
        let mut tx_outs: Vec<TxOut> = (0..index).map(|_| TxOut::null()).collect();
        tx_outs.push(copy_tx.vout[index].clone());
        copy_tx.vout = Vout::new(tx_outs);

        let mut vin = vec![];

        // let mut vin = copy_tx.vin.clone();
        for i in 0..copy_tx.vin.items().len() {
            let mut txin = copy_tx.vin[i].clone();
            if i != index { txin.sequence = 0; }
            vin.push(txin);
        }
        copy_tx.vin = vin.into();
        Ok(())
    }

    fn legacy_sighash_anyone_can_pay(
        copy_tx: &mut Self,
        index: usize) -> TxResult<()>
    {
        copy_tx.vin = Vin::new(vec![copy_tx.vin[index].clone()]);
        Ok(())
    }
}

impl<'a> Transaction<'a> for LegacyTx {
    type TxIn = TxIn;
    type TxOut = TxOut;
    type SighashArgs = LegacySighashArgs<'a>;
    type TXID = TXID;

    fn inputs(&'a self) -> &'a[Self::TxIn] {
        &self.vin.items()
    }

    fn outputs(&'a self) -> &'a[Self::TxOut] {
        &self.vout.items()
    }

    fn version(&self) -> u32 {
        self.version
    }

    fn locktime(&self) -> u32 {
        self.locktime
    }

    fn txid(&self) -> Self::TXID {
        let mut w = Hash256Writer::default();
        self.serialize(&mut w).expect("No IOError from SHA2");
        w.finish_marked()
    }

    fn write_legacy_sighash_preimage<W: Write>(
        &self,
        writer: &mut W,
        args: &LegacySighashArgs
    ) -> TxResult<()> {
        let mut copy_tx: Self = self.legacy_sighash_prep(args.index, args.prevout_script);
        if args.sighash_flag == Sighash::Single || args.sighash_flag == Sighash::SingleACP {
            Self::legacy_sighash_single(
                &mut copy_tx,
                args.index
            )?;
        }

        if args.sighash_flag as u8 & 0x80 == 0x80 {
            Self::legacy_sighash_anyone_can_pay(&mut copy_tx, args.index)?;
        }

        copy_tx.serialize(writer)?;
        (args.sighash_flag as u32).serialize(writer)?;

        Ok(())
    }
}

impl Ser for LegacyTx
{
    fn serialized_length(&self) -> usize {
        let mut len = self.version().serialized_length();
        len += self.vin.serialized_length();
        len += self.vout.serialized_length();
        len += self.locktime().serialized_length();
        len
    }

    fn deserialize<R>(reader: &mut R, _limit: usize) -> TxResult<Self>
    where
        R: Read,
        Self: std::marker::Sized
    {
        let version = u32::deserialize(reader, 0)?;
        let vin = Vin::deserialize(reader, 0)?;
        let vout = Vout::deserialize(reader, 0)?;
        let locktime = u32::deserialize(reader, 0)?;
        Ok(Self{
            version,
            vin,
            vout,
            locktime,
        })
    }

    fn serialize<T>(&self, writer: &mut T) -> TxResult<usize>
    where
        T: Write
    {
        let mut len = self.version().serialize(writer)?;
        len += self.vin.serialize(writer)?;
        len += self.vout.serialize(writer)?;
        len += self.locktime().serialize(writer)?;
        Ok(len)
    }
}

trait WitnessTransaction<'a>: Transaction<'a> {
    type WTXID: MarkedHash<Hash256Digest>;
    type WitnessSighashArgs;

    fn wtxid(&self) -> Self::WTXID;
    fn write_witness_sighash_preimage<W: Write>(&self, _writer: &mut W, args: &Self::WitnessSighashArgs) -> TxResult<()>;
    fn witness_sighash(&self, args: &Self::WitnessSighashArgs) -> Hash256Digest {
        let mut w = Hash256Writer::default();
        self.write_witness_sighash_preimage(&mut w, args).expect("No IOError from SHA2");
        w.finish()
    }
}

/// The arguments to the BIP143 sighash function
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WitnessSighashArgs<'a> {
    /// The index of the input we'd like to sign
    pub index: usize,
    /// The sighash mode to use.
    pub sighash_flag: Sighash,
    /// The script used in the prevout, which must be signed.
    pub prevout_script: &'a Script,
    /// The value of the prevout
    pub prevout_value: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WitnessTx {
    version: u32,
    vin: Vin,
    vout: Vout,
    witnesses: Vec<Witness>,
    locktime: u32,
}

impl WitnessTx {
    pub fn without_witness(&self) -> LegacyTx {
        LegacyTx {
            version: self.version,
            vin: self.vin.clone(),
            vout: self.vout.clone(),
            locktime: self.locktime
        }
    }

    fn hash_prevouts(&self, sighash_flag: Sighash) -> TxResult<Hash256Digest> {
        if sighash_flag as u8 & 0x80 == 0x80 {
            Ok(Hash256Digest::default())
        } else {
            let mut w = Hash256Writer::default();
            for input in self.vin.items().iter() {
                input.outpoint.serialize(&mut w)?;
            }
            Ok(w.finish())
        }

    }

    fn hash_sequence(&self, sighash_flag: Sighash) -> TxResult<Hash256Digest> {
        if sighash_flag == Sighash::Single || sighash_flag as u8 & 0x80 == 0x80 {
            Ok(Hash256Digest::default())
        } else {
            let mut w = Hash256Writer::default();
            for input in self.vin.items().iter() {
                input.sequence.serialize(&mut w)?;
            }
            Ok(w.finish())
        }
    }

    fn hash_outputs(&self, index: usize, sighash_flag: Sighash) -> TxResult<Hash256Digest> {
        match sighash_flag {
            Sighash::All | Sighash::AllACP  => {
                let mut w = Hash256Writer::default();
                for output in self.vout.items().iter() {
                    output.serialize(&mut w)?;
                }
                Ok(w.finish())
            },
            Sighash::Single | Sighash::SingleACP => {
                let mut w = Hash256Writer::default();
                self.vout[index].serialize(&mut w)?;
                Ok(w.finish())
            },
            _ => Ok(Hash256Digest::default())
        }
    }
}

impl<'a> Transaction<'a> for WitnessTx {
    type TxIn = TxIn;
    type TxOut = TxOut;
    type SighashArgs = LegacySighashArgs<'a>;
    type TXID = TXID;

    fn inputs(&'a self) -> &'a[Self::TxIn] {
        &self.vin.items()
    }

    fn outputs(&'a self) -> &'a[Self::TxOut] {
        &self.vout.items()
    }

    fn version(&self) -> u32 {
        self.version
    }

    fn locktime(&self) -> u32 {
        self.locktime
    }

    fn txid(&self) -> Self::TXID {
        let mut w = Hash256Writer::default();
        self.version().serialize(&mut w).expect("No IOError from SHA2");
        self.vin.serialize(&mut w).expect("No IOError from SHA2");
        self.vout.serialize(&mut w).expect("No IOError from SHA2");
        self.locktime().serialize(&mut w).expect("No IOError from SHA2");
        w.finish_marked()
    }

    fn write_legacy_sighash_preimage<W: Write>(
        &self,
        writer: &mut W,
        args: &LegacySighashArgs
    ) -> TxResult<()> {
        self.without_witness().write_legacy_sighash_preimage(writer, args)
    }
}

impl<'a> WitnessTransaction<'a> for WitnessTx {
    type WTXID = WTXID;
    type WitnessSighashArgs = WitnessSighashArgs<'a>;

    fn wtxid(&self) -> Self::WTXID {
        let mut w = Hash256Writer::default();
        self.serialize(&mut w).expect("No IOError from SHA2");
        w.finish_marked()
    }

    fn write_witness_sighash_preimage<W>(
        &self,
        writer: &mut W,
        args: &WitnessSighashArgs) -> TxResult<()>
    where
        W: Write
    {
        let input = &self.vin[args.index];

        self.version.serialize(writer)?;
        self.hash_prevouts(args.sighash_flag)?.serialize(writer)?;
        self.hash_sequence(args.sighash_flag)?.serialize(writer)?;
        input.outpoint.serialize(writer)?;
        args.prevout_script.serialize(writer)?;
        args.prevout_value.serialize(writer)?;
        input.sequence.serialize(writer)?;
        self.hash_outputs(args.index, args.sighash_flag)?.serialize(writer)?;
        self.locktime.serialize(writer)?;
        (args.sighash_flag as u32).serialize(writer)?;
        Ok(())
    }
}

impl Ser for WitnessTx
{
    fn serialized_length(&self) -> usize {
        let mut len = self.version().serialized_length();
        len += 2;  // Segwit Flag
        len += self.vin.serialized_length();
        len += self.vout.serialized_length();
        len += self.witnesses.serialized_length();
        len += self.locktime().serialized_length();
        len
    }

    fn deserialize<R>(reader: &mut R, _limit: usize) -> TxResult<Self>
    where
        R: Read,
        Self: std::marker::Sized
    {
        let version = u32::deserialize(reader, 0)?;
        let mut flag = [0u8; 2];
        reader.read_exact(&mut flag)?;
        if flag != [0u8, 1u8] { return Err(TxError::BadWitnessFlag(flag)); };
        let vin = Vin::deserialize(reader, 0)?;
        let vout = Vout::deserialize(reader, 0)?;
        let witnesses = Vec::<Witness>::deserialize(reader, vin.len())?;
        let locktime = u32::deserialize(reader, 0)?;
        Ok(Self{
            version,
            vin,
            vout,
            witnesses,
            locktime,
        })
    }

    fn serialize<T>(&self, writer: &mut T) -> TxResult<usize>
    where
        T: Write
    {
        let mut len = self.version().serialize(writer)?;
        len += writer.write(&[0u8, 1u8])?;
        len += self.vin.serialize(writer)?;
        len += self.vout.serialize(writer)?;
        len += self.witnesses.serialize(writer)?;
        len += self.locktime().serialize(writer)?;
        Ok(len)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::*;

    #[test]
    fn it_calculates_legacy_sighashes_and_txids() {
        // pulled from riemann helpers
        let tx_hex = "0100000001813f79011acb80925dfe69b3def355fe914bd1d96a3f5f71bf8303c6a989c7d1000000006b483045022100ed81ff192e75a3fd2304004dcadb746fa5e24c5031ccfcf21320b0277457c98f02207a986d955c6e0cb35d446a89d3f56100f4d7f67801c31967743a9c8e10615bed01210349fc4e631e3624a545de3f89f5d8684c7b8138bd94bdd531d2e213bf016b278afeffffff02a135ef01000000001976a914bc3b654dca7e56b04dca18f2566cdaf02e8d9ada88ac99c39800000000001976a9141c4bc762dd5423e332166702cb75f40df79fea1288ac19430600";
        let tx = LegacyTx::deserialize_hex(tx_hex.to_owned()).unwrap();

        let prevout_script_hex = "17a91424d6008f143af0cca57344069c46661aa4fcea2387";
        let prevout_script = Script::deserialize_hex(prevout_script_hex.to_owned()).unwrap();

        let all = Hash256Digest::deserialize_hex("b85c4f8d1377cc138225dd9b319d0a4ca547f7884270640f44c5fcdf269e0fe8".to_owned()).unwrap();
        let all_anyonecanpay = Hash256Digest::deserialize_hex("3b67a5114cc9fc837ddd6f6ec11bde38db5f68c34ab6ece2a043d7b25f2cf8bb".to_owned()).unwrap();
        let single = Hash256Digest::deserialize_hex("1dab67d768be0380fc800098005d1f61744ffe585b0852f8d7adc12121a86938".to_owned()).unwrap();
        let single_anyonecanpay = Hash256Digest::deserialize_hex("d4687b93c0a9090dc0a3384cd3a594ce613834bb37abc56f6032e96c597547e3".to_owned()).unwrap();

        let txid = Hash256Digest::deserialize_hex("03ee4f7a4e68f802303bc659f8f817964b4b74fe046facc3ae1be4679d622c45".to_owned()).unwrap();
        assert_eq!(tx.txid(), txid.into());

        let mut args = LegacySighashArgs {
            index: 0,
            sighash_flag: Sighash::All,
            prevout_script: &prevout_script,
        };

        assert_eq!(tx.legacy_sighash(&args), all);
        args.sighash_flag = Sighash::AllACP;
        assert_eq!(tx.legacy_sighash(&args), all_anyonecanpay);
        args.sighash_flag = Sighash::Single;
        assert_eq!(tx.legacy_sighash(&args), single);
        args.sighash_flag = Sighash::SingleACP;
        assert_eq!(tx.legacy_sighash(&args), single_anyonecanpay);
    }

    #[test]
    fn it_calculates_witness_sighashes_and_txids() {
        // pulled from riemann helpers
        let tx_hex = "02000000000101ee9242c89e79ab2aa537408839329895392b97505b3496d5543d6d2f531b94d20000000000fdffffff0173d301000000000017a914bba5acbec4e6e3374a0345bf3609fa7cfea825f18700cafd0700";
        let tx = WitnessTx::deserialize_hex(tx_hex.to_owned()).unwrap();

        let prevout_script_hex = "160014758ce550380d964051086798d6546bebdca27a73";
        let prevout_script = Script::deserialize_hex(prevout_script_hex.to_owned()).unwrap();

        let all = Hash256Digest::deserialize_hex("135754ab872e4943f7a9c30d6143c4c7187e33d0f63c75ec82a7f9a15e2f2d00".to_owned()).unwrap();
        let all_anyonecanpay = Hash256Digest::deserialize_hex("cc7438d5b15e93ba612dcd227cf1937c35273675b3aa7d1b771573667376ddf6".to_owned()).unwrap();
        let single = Hash256Digest::deserialize_hex("d04631d2742e6fd8e80e2e4309dece65becca41d37fd6bc0bcba041c52d824d5".to_owned()).unwrap();
        let single_anyonecanpay = Hash256Digest::deserialize_hex("ffea9cdda07170af9bc9967cedf485e9fe15b78a622e0c196c0b6fc64f40c615".to_owned()).unwrap();

        let txid = Hash256Digest::deserialize_hex("9e77087321b870859ebf08976d665c42d9f98cad18fff6a05a91c1d2da6d6c41".to_owned()).unwrap();
        assert_eq!(tx.txid(), txid.into());

        let mut args = WitnessSighashArgs {
            index: 0,
            sighash_flag: Sighash::All,
            prevout_script: &prevout_script,
            prevout_value: 120000,
        };

        assert_eq!(tx.witness_sighash(&args), all);

        args.sighash_flag = Sighash::AllACP;
        assert_eq!(tx.witness_sighash(&args), all_anyonecanpay);

        args.sighash_flag = Sighash::Single;
        assert_eq!(tx.witness_sighash(&args), single);

        args.sighash_flag = Sighash::SingleACP;
        assert_eq!(tx.witness_sighash(&args), single_anyonecanpay);
    }

    #[test]
    fn it_passes_more_witness_sighash_tests() {
        // from riemann
        let tx_hex = "02000000000102ee9242c89e79ab2aa537408839329895392b97505b3496d5543d6d2f531b94d20000000000fdffffffee9242c89e79ab2aa537408839329895392b97505b3496d5543d6d2f531b94d20000000000fdffffff0273d301000000000017a914bba5acbec4e6e3374a0345bf3609fa7cfea825f18773d301000000000017a914bba5acbec4e6e3374a0345bf3609fa7cfea825f1870000cafd0700";
        let tx = WitnessTx::deserialize_hex(tx_hex.to_owned()).unwrap();

        let prevout_script_hex = "160014758ce550380d964051086798d6546bebdca27a73";
        let prevout_script = Script::deserialize_hex(prevout_script_hex.to_owned()).unwrap();

        let all = Hash256Digest::deserialize_hex("75385c87ece4980b581cfd71bc5814f607801a87f6e0973c63dc9fda465c19c4".to_owned()).unwrap();
        let all_anyonecanpay = Hash256Digest::deserialize_hex("bc55c4303c82cdcc8e290c597a00d662ab34414d79ec15d63912b8be7fe2ca3c".to_owned()).unwrap();
        let single = Hash256Digest::deserialize_hex("9d57bf7af01a4e0baa57e749aa193d37a64e3bbc08eb88af93944f41af8dfc70".to_owned()).unwrap();
        let single_anyonecanpay = Hash256Digest::deserialize_hex("ffea9cdda07170af9bc9967cedf485e9fe15b78a622e0c196c0b6fc64f40c615".to_owned()).unwrap();

        let txid = Hash256Digest::deserialize_hex("184e7bce099679b27ed958213c97d2fb971e227c6517bca11f06ccbb97dcdc30".to_owned()).unwrap();
        assert_eq!(tx.txid(), txid.into());

        let mut args = WitnessSighashArgs {
            index: 1,
            sighash_flag: Sighash::All,
            prevout_script: &prevout_script,
            prevout_value: 120000,
        };

        assert_eq!(tx.witness_sighash(&args), all);

        args.sighash_flag = Sighash::AllACP;
        assert_eq!(tx.witness_sighash(&args), all_anyonecanpay);

        args.sighash_flag = Sighash::Single;
        assert_eq!(tx.witness_sighash(&args), single);

        args.sighash_flag = Sighash::SingleACP;
        assert_eq!(tx.witness_sighash(&args), single_anyonecanpay);
    }

    #[test]
    fn it_passes_more_legacy_sighash_tests() {
        // from riemann
        let tx_hex = "0200000002ee9242c89e79ab2aa537408839329895392b97505b3496d5543d6d2f531b94d20000000000fdffffffee9242c89e79ab2aa537408839329895392b97505b3496d5543d6d2f531b94d20000000000fdffffff0273d301000000000017a914bba5acbec4e6e3374a0345bf3609fa7cfea825f18773d301000000000017a914bba5acbec4e6e3374a0345bf3609fa7cfea825f18700000000";
        let tx = LegacyTx::deserialize_hex(tx_hex.to_owned()).unwrap();

        let prevout_script_hex = "160014758ce550380d964051086798d6546bebdca27a73";
        let prevout_script = Script::deserialize_hex(prevout_script_hex.to_owned()).unwrap();

        let all = Hash256Digest::deserialize_hex("3ab40bf1287b7be9a5c67ed0f97f80b38c5f68e53ec93bffd3893901eaaafdb2".to_owned()).unwrap();
        let all_anyonecanpay = Hash256Digest::deserialize_hex("2d5802fed31e1ef6a857346cc0a9085ea452daeeb3a0b5afcb16a2203ce5689d".to_owned()).unwrap();
        let single = Hash256Digest::deserialize_hex("ea52b62b26c1f0db838c952fa50806fb8e39ba4c92a9a88d1b4ba7e9c094517d".to_owned()).unwrap();
        let single_anyonecanpay = Hash256Digest::deserialize_hex("9e2aca0a04afa6e1e5e00ff16b06a247a0da1e7bbaa7cd761c066a82bb3b07d0".to_owned()).unwrap();

        let txid = Hash256Digest::deserialize_hex("40157948972c5c97a2bafff861ee2f8745151385c7f9fbd03991ddf59b76ac81".to_owned()).unwrap();
        assert_eq!(tx.txid(), txid.into());

        let mut args = LegacySighashArgs {
            index: 1,
            sighash_flag: Sighash::All,
            prevout_script: &prevout_script,
        };


        assert_eq!(tx.legacy_sighash(&args), all);

        args.sighash_flag = Sighash::AllACP;
        assert_eq!(tx.legacy_sighash(&args), all_anyonecanpay);

        args.sighash_flag = Sighash::Single;
        assert_eq!(tx.legacy_sighash(&args), single);

        args.sighash_flag = Sighash::SingleACP;
        assert_eq!(tx.legacy_sighash(&args), single_anyonecanpay);
    }
}
