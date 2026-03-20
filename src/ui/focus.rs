#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FocusableField {
	// Online Account fields
	Username,
	Email,
	Phone,
	Password,
	Website,
	Status,
	TwoFactor,
	SignInProviders,
	DateCreated,
	SecurityQuestions,
	Notes,
	// Social Security fields
	AccountNumber,
	LegalName,
	Country,
	IssuanceDate,
}
