use crate::span::Span;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Program {
    pub imports: Vec<ImportDecl>,
    pub functions: Vec<FunctionDecl>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportDecl {
    pub domain: Identifier,
    pub library: Identifier,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FunctionDecl {
    pub return_type: TypeSyntax,
    pub name: Identifier,
    pub parameters: Vec<Parameter>,
    pub body: BlockStmt,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Parameter {
    pub name: Identifier,
    pub ty: TypeSyntax,
    pub conversion: ConversionMode,
    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConversionMode {
    Exact,
    Auto,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Identifier {
    pub name: String,
    pub span: Span,
}

/// Maximum number of array dimensions a type can carry.
pub const MAX_ARRAY_RANK: usize = 8;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeSyntax {
    pub kind: TypeKind,
    /// One entry per dimension, innermost first. `None` is a dynamic array.
    pub bounds: Vec<Option<u64>>,
    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TypeKind {
    Void,
    Int,
    UInt,
    Flt,
    Int16,
    Int32,
    Int64,
    UInt16,
    UInt32,
    UInt64,
    Flt32,
    Flt64,
    Bln,
    Chr,
    Str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockStmt {
    pub statements: Vec<Stmt>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Stmt {
    Block(BlockStmt),
    VariableDecl(VariableDecl),
    Destructure(DestructureDecl),
    Return(ReturnStmt),
    If(IfStmt),
    Loop(LoopStmt),
    Break(Span),
    Continue(Span),
    Expression(ExpressionStmt),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DestructureDecl {
    pub bindings: Vec<BindingHead>,
    pub initializer: Expr,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BindingHead {
    pub name: Identifier,
    pub ty: TypeSyntax,
    pub conversion: ConversionMode,
    pub mutable: bool,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VariableDecl {
    pub name: Identifier,
    pub ty: TypeSyntax,
    pub conversion: ConversionMode,
    pub initializer: Expr,
    pub mutable: bool,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReturnStmt {
    pub value: Option<Expr>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExpressionStmt {
    pub expression: Expr,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IfStmt {
    pub condition: Expr,
    pub then_branch: BlockStmt,
    pub else_branch: Option<Box<Stmt>>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoopStmt {
    pub kind: LoopKind,
    pub body: BlockStmt,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoopKind {
    Infinite,
    Condition {
        condition: Expr,
    },
    CStyle {
        initializer: Option<VariableDecl>,
        condition: Option<Expr>,
        update: Option<Expr>,
    },
    Iteration {
        bindings: Vec<BindingHead>,
        iterable: Expr,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Expr {
    Identifier(Identifier),
    Literal(LiteralExpr),
    Unary(UnaryExpr),
    Binary(BinaryExpr),
    Assignment(AssignmentExpr),
    Call(CallExpr),
    Conversion(ConversionExpr),
    Member(MemberExpr),
    Update(UpdateExpr),
    InterpolatedString(InterpolatedStringExpr),
    Array(ArrayExpr),
    Index(IndexExpr),
    Slice(SliceExpr),
    Parenthesized { expression: Box<Expr>, span: Span },
}

impl Expr {
    pub fn span(&self) -> Span {
        match self {
            Expr::Identifier(identifier) => identifier.span,
            Expr::Literal(literal) => literal.span(),
            Expr::Unary(unary) => unary.span,
            Expr::Binary(binary) => binary.span,
            Expr::Assignment(assignment) => assignment.span,
            Expr::Call(call) => call.span,
            Expr::Conversion(conversion) => conversion.span,
            Expr::Member(member) => member.span,
            Expr::Update(update) => update.span,
            Expr::InterpolatedString(interpolated) => interpolated.span,
            Expr::Array(array) => array.span,
            Expr::Index(index) => index.span,
            Expr::Slice(slice) => slice.span,
            Expr::Parenthesized { span, .. } => *span,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LiteralExpr {
    Integer { lexeme: String, span: Span },
    Float { lexeme: String, span: Span },
    Boolean { value: bool, span: Span },
    String { value: String, span: Span },
}

impl LiteralExpr {
    pub fn span(&self) -> Span {
        match self {
            LiteralExpr::Integer { span, .. }
            | LiteralExpr::Float { span, .. }
            | LiteralExpr::Boolean { span, .. }
            | LiteralExpr::String { span, .. } => *span,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArrayExpr {
    pub elements: Vec<Expr>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexExpr {
    pub object: Box<Expr>,
    pub index: Box<Expr>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SliceExpr {
    pub object: Box<Expr>,
    pub start: Option<Box<Expr>>,
    pub stop: Option<Box<Expr>>,
    pub step: Option<Box<Expr>>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnaryExpr {
    pub operator: UnaryOperator,
    pub operand: Box<Expr>,
    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnaryOperator {
    Negate,
    Not,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BinaryExpr {
    pub left: Box<Expr>,
    pub operator: BinaryOperator,
    pub right: Box<Expr>,
    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinaryOperator {
    Add,
    Subtract,
    Multiply,
    Divide,
    Remainder,
    Equal,
    NotEqual,
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
    And,
    Or,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssignmentExpr {
    pub target: Box<Expr>,
    pub value: Box<Expr>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CallExpr {
    pub callee: Box<Expr>,
    pub arguments: Vec<Expr>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConversionExpr {
    pub target: TypeSyntax,
    pub arguments: Vec<Expr>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemberExpr {
    pub object: Box<Expr>,
    pub member: Identifier,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdateExpr {
    pub operator: UpdateOperator,
    pub operand: Box<Expr>,
    pub position: UpdatePosition,
    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpdateOperator {
    Increment,
    Decrement,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpdatePosition {
    Prefix,
    Postfix,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InterpolatedStringExpr {
    pub parts: Vec<StringPart>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StringPart {
    Text { value: String, span: Span },
    Expression { expression: Expr, span: Span },
}
