# PRIVACY POLICY

**Effective Date:** 15.07.2025

**Last Updated:** 15.07.2025

## 1. GENERAL PROVISIONS

### 1.1. Introduction

This Privacy Policy (hereinafter — "Policy") describes how **ZeroID: Absolutely Secret Chat** application (hereinafter — "Application", "we", "us", "our") collects, uses, stores, and protects user information.

### 1.2. Acceptance of Policy

By using the Application, you agree to the terms of this Policy. If you do not agree with any provisions, please do not use the Application.

### 1.3. Changes to Policy

We reserve the right to modify this Policy. We will notify users of any material changes through the Application or on our website.

## 2. APPLICATION DESCRIPTION

### 2.1. Functionality

ZeroID is a decentralized secure messaging application that provides:
- Peer-to-peer (P2P) connections without central servers
- End-to-end encryption of all messages
- Anonymous usage without registration
- No storage of message history on third-party servers

### 2.2. Technical Specifications

The Application uses the following security technologies:
- WebRTC for P2P connections
- X25519 for key exchange
- ChaCha20-Poly1305 for message encryption
- HKDF for key generation
- SAS (Short Authentication String) for man-in-the-middle attack protection

## 3. INFORMATION COLLECTION

### 3.1. Information We Do NOT Collect

**Important:** ZeroID is designed with the principle of minimal data collection. We do NOT collect:

- Personal user data (name, email, phone)
- Message content
- Chat history
- IP addresses (except as described below)
- Message metadata
- Location information
- Application usage data

### 3.2. Information That May Be Collected

#### 3.2.1. Technical Information
- Operating system version
- Application version
- Device compatibility information
- Error logs (only in case of application crashes)

#### 3.2.2. Network Information
- Temporary IP addresses (only for establishing P2P connection)
- WebRTC data for connection setup
- ICE (Interactive Connectivity Establishment) parameters

**Note:** All network information is used exclusively for establishing direct connections between users and is not stored.

### 3.3. Locally Generated Information

The Application generates the following data locally on the user's device:
- Cryptographic keys for encryption
- Temporary session identifiers
- QR codes for connection establishment
- Temporary message buffers

**Important:** All this information is automatically deleted when the application is closed or the session ends.

## 4. USE OF INFORMATION

### 4.1. Purposes of Use

Collected information is used exclusively for:
- Ensuring application functionality
- Establishing P2P connections between users
- Ensuring security and encryption
- Resolving technical issues

### 4.2. Usage Limitations

We do NOT use collected information for:
- Targeted advertising
- User behavior analysis
- Transfer to third parties
- Commercial purposes

## 5. DATA STORAGE AND SECURITY

### 5.1. Storage Principles

- **Local Storage:** All data is stored locally on the user's device
- **Temporary Storage:** Cryptographic keys and session data are deleted after session completion
- **No Central Storage:** We do not have access to user messages

### 5.2. Security Measures

#### 5.2.1. Cryptographic Protection
- End-to-end encryption of all messages
- Use of proven cryptographic algorithms
- Automatic deletion of keys from memory
- Protection against man-in-the-middle attacks

#### 5.2.2. Technical Protection
- Zeroization of sensitive data in memory
- Protection against buffer overflow
- Limitation of incoming data size
- Automatic state cleanup on disconnect

### 5.3. Retention Periods

- **Messages:** Not stored on servers
- **Encryption Keys:** Deleted immediately after use
- **Session Data:** Deleted upon session completion
- **Technical Logs:** Stored locally and can be deleted by the user

## 6. DATA TRANSFER

### 6.1. Transfer Between Users

Messages are transmitted directly between users through P2P connection. We do not have access to the content of these messages.

### 6.2. Transfer to Third Parties

We do NOT transfer data to third parties, except in cases of:
- Legal requirements
- Protection of user rights and safety
- Prevention of fraud or illegal activity

### 6.3. International Transfer

Since the application works locally on the user's device, international data transfer does not occur.

## 7. USER RIGHTS

### 7.1. Right to Information

Users have the right to:
- Receive information about what data is collected
- Learn how their data is used
- Receive a copy of collected data (if any)

### 7.2. Right to Deletion

Users can:
- Delete all local application data
- End the session at any time
- Remove the application from their device

### 7.3. Right to Withdraw Consent

Users can stop using the application at any time, which automatically stops processing their data.

## 8. CHILDREN'S PRIVACY

### 8.1. Age Restrictions

The Application is not intended for children under 13 years of age. We do not knowingly collect information from children under 13.

### 8.2. Parental Control

Parents and guardians should monitor their children's use of the application and explain to them the principles of safe communication on the internet.

## 9. TECHNICAL ASPECTS

### 9.1. WebRTC and P2P Connections

- The Application uses WebRTC to establish direct connections
- Temporary STUN/TURN servers may be used to bypass NAT
- These servers do not have access to message content

### 9.2. Cryptographic Algorithms

- **X25519:** For key exchange
- **ChaCha20-Poly1305:** For message encryption
- **HKDF-SHA256:** For key generation
- **SHA-256:** For SAS creation

### 9.3. Security Limitations

Users should be aware of the following limitations:
- Security depends on proper SAS comparison
- User's device may be compromised
- Connection metadata may be visible to network administrators

## 10. DISCLAIMER

### 10.1. Limitation of Liability

We are not responsible for:
- Improper use of the application
- Compromise of user's device
- Actions of third parties
- Data loss due to technical failures

### 10.2. "As Is"

The Application is provided "as is" without any warranties.

## 11. CONTACT INFORMATION

### 11.1. Privacy Questions

For all privacy-related questions, contact us at:
- Email: valdimir.developer@bk.ru
- GitHub: github.com/vovakirdan/ssc

### 11.2. Complaints

If you believe your privacy rights have been violated, you may:
- Contact us directly
- Contact the relevant data protection authorities

## 12. LEGAL INFORMATION

### 12.1. Governing Law

This Policy is governed by the laws of Russian Federation.

### 12.2. Legislative Changes

In case of changes in data protection legislation, we will update the Policy in accordance with new requirements.

## 13. FINAL PROVISIONS

### 13.1. Complete Agreement

This Policy represents the complete agreement between the user and us regarding personal data processing.

### 13.2. Severability

If any provision of the Policy is found to be invalid, the remaining provisions remain in force.

### 13.3. Headings

Section headings are for convenience only and do not affect the interpretation of the Policy.

---

**Note:** This Privacy Policy is drafted taking into account the specifics of a decentralized P2P application. The main principle is minimal data collection and maximum protection of user privacy.
