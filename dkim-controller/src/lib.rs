use mailparse::{addrparse_header, parse_mail, MailHeaderMap};
use near_dkim::dns::Lookup;
use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::serde::Serialize;
use near_sdk::store::LookupMap;
use near_sdk::{
    env, near_bindgen, require, AccountId, Balance, Gas, GasWeight, PanicOnDefault, Promise,
    PublicKey, ONE_NEAR,
};

use near_dkim::verify_email_with_resolver;

pub fn always_fail(_: &mut [u8]) -> Result<(), getrandom::Error> {
    unimplemented!()
}

use getrandom::register_custom_getrandom;
register_custom_getrandom!(always_fail);

// Define the contract structure
#[near_bindgen]
#[derive(BorshDeserialize, BorshSerialize, PanicOnDefault)]
pub struct DkimController {
    resolver: DkimResolver,
}

#[derive(BorshDeserialize, BorshSerialize)]
struct DkimResolver {
    map: LookupMap<String, String>,
}

impl Lookup for DkimResolver {
    fn lookup_txt(&self, name: &str) -> Result<Vec<String>, near_dkim::DKIMError> {
        match self.map.get(name) {
            Some(dkim) => Ok(vec![dkim.clone()]),
            None => Err(near_dkim::DKIMError::KeyUnavailable(
                "unknown domain".to_string(),
            )),
        }
    }
}

const MIN_STORAGE: Balance = 4_200_000_000_000_000_000_000_000; //11.1Ⓝ
const ACCESS_DELEGATOR_CODE: &[u8] = include_bytes!("access_delegator.wasm");

#[derive(Debug, PartialEq)]
pub enum CommandEnum {
    Init,
    AddKey(PublicKey),
    DeleteKey,
    Transfer(AccountId, Balance),
}

#[derive(Serialize)]
#[serde(crate = "near_sdk::serde")]
struct AddKeyArgs {
    public_key: PublicKey,
}

#[derive(Serialize)]
#[serde(crate = "near_sdk::serde")]
struct TransferArgs {
    to: AccountId,
    amount: Balance,
}

#[derive(Serialize)]
#[serde(crate = "near_sdk::serde")]
struct NewContractArgs {
    controller_id: AccountId,
}

// Implement the contract structure
#[near_bindgen]
impl DkimController {
    #[init]
    pub fn new() -> Self {
        let mut map = LookupMap::new(b"a");
        map.insert(
            "20210112._domainkey.gmail.com".to_owned(), "v=DKIM1; k=rsa; p=MIIBIjANBgkqhkiG9w0BAQEFAAOCAQ8AMIIBCgKCAQEAq8JxVBMLHZRj1WvIMSHApRY3DraE/EiFiR6IMAlDq9GAnrVy0tDQyBND1G8+1fy5RwssQ9DgfNe7rImwxabWfWxJ1LSmo/DzEdOHOJNQiP/nw7MdmGu+R9hEvBeGRQ Amn1jkO46KIw/p2lGvmPSe3+AVD+XyaXZ4vJGTZKFUCnoctAVUyHjSDT7KnEsaiND2rVsDvyisJUAH+EyRfmHSBwfJVHAdJ9oD8cn9NjIun/EHLSIwhCxXmLJlaJeNAFtcGeD2aRGbHaS7M6aTFP+qk4f2ucRx31cyCxbu50CDVfU+d4JkIDNBFDiV+MIpaDFXIf11bGoS08oBBQiyPXgX0wIDAQAB".to_owned());
        map.insert(
            "google._domainkey.near.org".to_owned(), "v=DKIM1; k=rsa; p=MIIBIjANBgkqhkiG9w0BAQEFAAOCAQ8AMIIBCgKCAQEAvp9AC5ykeX9XfNDcv3lKLft21MpXUTb45fOvSyjArMjmVCJT8mQCkehardajVAFvcBYOk0I9DJtvclvFnDBYV8T69HMGzCmuIibHrw4ImB+VCwLFk7M7lsBgSo5FDS1z8swgMyTsKKFmsLOFmvMXwF+arLIQRNYLwTs/JyPl6ExjQJqfNhVu/A1SqAc2wm1Tg n2i0m+9oj0HI5HZ5VX23T4f2Aew2AxascByQx6ue47avziBtV9c84IpnpFTbrozPkXWKlyjXEY9YArw6LqKg1mn7iQAWoeVQOvC8Kv6O2CVCw+RCLzHiZs8lpu/vwtyJ8hhNoI+tJLKm/Va5C9ZnwIDAQAB".to_owned());
        map.flush();

        Self {
            resolver: DkimResolver { map },
        }
    }

    fn create_new_subaccount(account_id: AccountId) {
        let create_args = near_sdk::serde_json::to_vec(&NewContractArgs {
            controller_id: env::current_account_id(),
        })
        .unwrap();

        Promise::new(account_id)
            .create_account()
            .transfer(MIN_STORAGE)
            .deploy_contract(ACCESS_DELEGATOR_CODE.to_vec())
            .function_call_weight(
                "set_controller".to_owned(),
                create_args,
                0,
                Gas(0),
                GasWeight(1),
            );
    }

    fn add_key(account_id: AccountId, public_key: PublicKey) {
        let add_key_args = near_sdk::serde_json::to_vec(&AddKeyArgs { public_key }).unwrap();

        Promise::new(account_id).function_call_weight(
            "add_key".to_owned(),
            add_key_args,
            0,
            Gas(0),
            GasWeight(1),
        );
    }

    fn transfer(account_id: AccountId, to: AccountId, amount: Balance) {
        let transfer_args = near_sdk::serde_json::to_vec(&TransferArgs { to, amount }).unwrap();

        Promise::new(account_id).function_call_weight(
            "transfer".to_owned(),
            transfer_args,
            0,
            Gas(0),
            GasWeight(1),
        );
    }

    fn verify_email(&self, full_email: Vec<u8>) -> (String, String) {
        let email = parse_mail(full_email.as_slice())
            .unwrap_or_else(|err| env::panic_str(&format!("The email is malformed: {}", err)));

        let result = verify_email_with_resolver(&email, &self.resolver)
            .unwrap_or_else(|err| env::panic_str(&format!("Email verification failed: {}", err)));
        require!(
            result.summary() == "pass",
            "Email signature does not match its contents"
        );

        let headers = email.get_headers();
        let from_header = headers
            .get_first_header("From")
            .unwrap_or_else(|| env::panic_str("The email lacks \"From\" header"));
        let mut from_list = addrparse_header(from_header).unwrap_or_else(|err| {
            env::panic_str(&format!("The email \"From\" header is malformed: {}", err))
        });
        require!(
            from_list.len() == 1,
            "The email \"From\" header contains more than one author"
        );
        let from = from_list.swap_remove(0);

        let addr = match from {
            mailparse::MailAddr::Single(single_email) => single_email.addr,
            _ => env::panic_str("The email \"From\" header contains a group of addresses"),
        };

        let subject = match email.get_headers().get_first_header("Subject") {
            Some(subject_header) => subject_header.get_value(),
            None => env::panic_str("The email lacks \"From\" header"),
        };

        (addr, subject)
    }

    fn validate_key(key: &str) -> PublicKey {
        require!(key.len() == 52, "The key length is not 52");
        let key_suffix = key
            .strip_prefix("ed25519:")
            .unwrap_or_else(|| env::panic_str("The key prefix does not match \"ed25519:\""));

        key_suffix.chars().for_each(|x| match x {
            'A'..='Z' | 'a'..='z' | '0'..='9' => (),
            _ => env::panic_str(&format!("The key contains an invalid character '{}'", x)),
        });

        key.parse()
            .unwrap_or_else(|err| env::panic_str(&format!("The key is malformed: {}", err)))
    }

    fn parse_command(subject: String) -> CommandEnum {
        let cmds: Vec<&str> = subject.split_whitespace().collect();
        match cmds.as_slice() {
            ["init"] => CommandEnum::Init,
            ["add_key", key] => CommandEnum::AddKey(DkimController::validate_key(key)),
            // TODO: Unsupported for now, this command should accept a public key and delete it
            ["delete_key"] => CommandEnum::DeleteKey,
            ["transfer", account, amount] => {
                let amount: f64 = amount.parse().unwrap_or_else(|err| {
                    env::panic_str(&format!(
                        "Failed to transfer due to malformed amount: {}",
                        err
                    ))
                });
                require!(amount > 0.0, "Transfer amount must be positive");
                let amount = ((amount * 100.0) as u128) * (ONE_NEAR / 100);

                let account: AccountId = account.parse().unwrap_or_else(|err| {
                    env::panic_str(&format!(
                        "Failed to transfer due to malformed account: {}",
                        err
                    ))
                });

                CommandEnum::Transfer(account, amount)
            }
            _ => env::panic_str("Unrecognized subject"),
        }
    }

    fn sender_to_account(sender: String) -> String {
        sender
            .chars()
            .map(|x| match x {
                'a'..='z' => x,
                'A'..='Z' => x,
                '0'..='9' => x,
                '@' => '_',
                '.' => '_',
                '_' => x,
                '-' => x,
                _ => env::panic_str(&format!(
                    "The sender email contains an unsupported character '{}'",
                    x
                )),
            })
            .collect()
    }

    pub fn receive_email(&self, full_email: Vec<u8>) {
        // verify email
        let (sender, header) = self.verify_email(full_email);
        env::log_str(format!("Email verified: {}", sender).as_str());
        let prefix = DkimController::sender_to_account(sender);
        env::log_str(format!("Account prefix is: {}", prefix).as_str());
        let account_id: AccountId = (prefix + "." + env::current_account_id().as_ref())
            .parse()
            .unwrap_or_else(|_| {
                env::panic_str("Unexpected error: failed to derive a valid account id")
            });
        let cmd = DkimController::parse_command(header);
        match cmd {
            CommandEnum::Init => DkimController::create_new_subaccount(account_id),
            CommandEnum::AddKey(key) => DkimController::add_key(account_id, key),
            CommandEnum::Transfer(to, amount) => DkimController::transfer(account_id, to, amount),
            _ => todo!(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    pub fn test_parse_command() {
        assert_eq!(
            DkimController::parse_command("init".to_owned()),
            CommandEnum::Init
        );

        assert_eq!(
            DkimController::parse_command(
                "add_key ed25519:3tXAA9zf5YSLxYELSbxwhEvMd7h9itTfCcUfEc3QfPgD".to_owned()
            ),
            CommandEnum::AddKey(
                "ed25519:3tXAA9zf5YSLxYELSbxwhEvMd7h9itTfCcUfEc3QfPgD"
                    .parse()
                    .unwrap()
            )
        );
        assert_eq!(
            DkimController::parse_command(
                "add_key     ed25519:3tXAA9zf5YSLxYELSbxwhEvMd7h9itTfCcUfEc3QfPgD
                \n"
                .to_owned()
            ),
            CommandEnum::AddKey(
                "ed25519:3tXAA9zf5YSLxYELSbxwhEvMd7h9itTfCcUfEc3QfPgD"
                    .parse()
                    .unwrap()
            )
        );
        assert_eq!(
            DkimController::parse_command("transfer foobar.near 134".to_owned()),
            CommandEnum::Transfer("foobar.near".parse().unwrap(), 134 * ONE_NEAR)
        );
    }

    #[test]
    pub fn verify_email() {
        let auth_manager = DkimController::new();
        assert_eq!(
            auth_manager.verify_email(include_bytes!("message.eml").to_vec()),
            (
                "example.near@gmail.com".to_owned(),
                "Another message".to_owned()
            )
        );
        assert_eq!(
            auth_manager.verify_email(include_bytes!("empty_email.eml").to_vec()),
            (
                "example.near@gmail.com".to_owned(),
                "Empty email".to_owned()
            )
        );
    }
    #[test]
    #[should_panic]
    pub fn verify_invalid_email() {
        let auth_manager = DkimController::new();
        assert_eq!(
            auth_manager.verify_email(include_bytes!("invalid_message.eml").to_vec()),
            (
                "example.near@gmail.com".to_owned(),
                "Another message".to_owned()
            )
        );
    }
}
