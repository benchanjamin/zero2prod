use crate::domain::SubscriberEmail;
use crate::domain::SubscriberName;

pub struct NewSubscriber {
    // We are not using String for email because we want to enforce that the email is a valid email address
    pub email: SubscriberEmail,
    // We are using SubscriberName because we want to enforce that the name is a valid name
    pub name: SubscriberName,
}
